//! Binary-level tests for `main.rs`'s argument dispatch — the thin layer the crate's unit tests
//! (`tools/tests.rs`, handler-level) never exercise. Spawns the real `zzop-mcp` executable
//! (`CARGO_BIN_EXE_zzop-mcp`, built by cargo for integration tests), so exit codes and the
//! stdout/stderr split are pinned exactly as a shell sees them.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zzop-mcp"))
        .args(args)
        .output()
        .expect("zzop-mcp binary should spawn")
}

/// Like `run`, but from a chosen working directory — the lane that pins relative-path arguments
/// (`analyze .`, `endpoint <pattern> <relative dir>`) resolving against the invocation cwd.
fn run_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zzop-mcp"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("zzop-mcp binary should spawn")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn version_subcommand_and_flag_print_the_server_version_and_exit_zero() {
    // Both spellings print the exact `server::version()` value — the same string MCP `initialize`
    // reports as serverInfo.version, so the CLI and the protocol can never disagree. Test builds
    // never set the compile-time `ZZOP_RELEASE_VERSION`, so this pins the 0.0.0 dev fallback lane;
    // the release lane is exercised live by release CI (prebuild.yml).
    for arg in ["version", "--version"] {
        let out = run(&[arg]);
        assert!(out.status.success(), "`zzop-mcp {arg}` must exit 0");
        assert_eq!(
            stdout(&out).trim(),
            format!("zzop-mcp {}", zzop_mcp::server::version()),
            "`zzop-mcp {arg}` must print the server::version() value"
        );
        assert!(stderr(&out).is_empty(), "no stderr on success");
    }
}

#[test]
fn top_level_help_prints_the_usage_line_to_stdout_and_exits_zero() {
    // An explicit help REQUEST is the polite lane: usage on stdout, exit 0 — distinct from the
    // exit-2 stderr lane every malformed invocation takes.
    for arg in ["--help", "-h", "help"] {
        let out = run(&[arg]);
        assert!(out.status.success(), "`zzop-mcp {arg}` must exit 0");
        let text = stdout(&out);
        assert!(text.contains("usage:"), "`{arg}` got: {text}");
        assert!(text.contains("analyze"), "`{arg}` got: {text}");
        assert!(stderr(&out).is_empty(), "help is not an error");
    }
}

#[test]
fn analyze_help_flag_is_a_usage_error_never_a_path() {
    // The blind-test failure this pins: `analyze --help` used to be swallowed as a path and die
    // with "path does not exist: --help" (exit 1). A dash-shaped argument in a path position is a
    // usage error, exit 2.
    let out = run(&["analyze", "--help"]);
    assert_eq!(out.status.code(), Some(2), "usage errors exit 2");
    let err = stderr(&out);
    assert!(err.contains("usage: zzop-mcp analyze"), "got: {err}");
    assert!(
        !err.contains("does not exist"),
        "must never be treated as a path: {err}"
    );
}

#[test]
fn endpoint_flag_like_pattern_is_a_usage_error_never_a_pattern() {
    let out = run(&["endpoint", "-x", "a", "b"]);
    assert_eq!(out.status.code(), Some(2), "usage errors exit 2");
    assert!(
        stderr(&out).contains("usage: zzop-mcp endpoint"),
        "got: {}",
        stderr(&out)
    );
}

#[test]
fn no_args_usage_error_names_every_subcommand_including_version() {
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2), "usage errors exit 2");
    let err = stderr(&out);
    assert!(err.contains("usage:"), "got: {err}");
    assert!(err.contains("version"), "usage must name version: {err}");
    assert!(
        err.contains("endpoint <pattern> --config <path>"),
        "usage must name endpoint's --config form: {err}"
    );
}

#[test]
fn endpoint_config_flag_without_a_path_is_a_usage_error() {
    let out = run(&["endpoint", "users", "--config"]);
    assert_eq!(out.status.code(), Some(2), "usage errors exit 2");
    assert!(
        stderr(&out).contains("usage: zzop-mcp endpoint <pattern> --config"),
        "got: {}",
        stderr(&out)
    );
}

#[test]
fn endpoint_config_flag_with_trailing_paths_is_a_usage_error() {
    // Exactly ONE of paths/config — the check_endpoint tool's own argument contract, surfaced as a
    // usage error at the CLI layer.
    let out = run(&["endpoint", "users", "--config", "some.jsonc", "extra-path"]);
    assert_eq!(out.status.code(), Some(2), "usage errors exit 2");
    assert!(
        stderr(&out).contains("no extra paths"),
        "got: {}",
        stderr(&out)
    );
}

#[test]
fn contract_with_no_name_lists_every_embedded_resource() {
    // The terminal lane to the embedded authoring contracts: `contract` with no name lists all ten
    // (name + description + mime, human-readable lines) — a terminal user must never have to
    // reverse-engineer the config surface from error messages while the docs sit inside the binary.
    let out = run(&["contract"]);
    assert!(out.status.success(), "`zzop-mcp contract` must exit 0");
    let text = stdout(&out);
    for doc in zzop_mcp::embedded::CONTRACT_DOCS {
        assert!(
            text.contains(doc.name),
            "list must name {}: {text}",
            doc.name
        );
        assert!(
            text.contains(doc.mime),
            "list must show {}'s mime: {text}",
            doc.name
        );
    }
    assert!(stderr(&out).is_empty(), "no stderr on success");
}

#[test]
fn contract_with_a_name_prints_the_exact_embedded_bytes() {
    // `contract config-surface` prints the resource's raw bytes to stdout — byte-identical to the
    // embedded constant (pipe-safe: no banner, no trailing newline added) and parseable as JSON,
    // exactly what MCP `resources/read` serves for the same name.
    let out = run(&["contract", "config-surface"]);
    assert!(
        out.status.success(),
        "`zzop-mcp contract config-surface` must exit 0"
    );
    assert_eq!(
        out.stdout,
        zzop_config::CONFIG_SURFACE_JSON.as_bytes(),
        "stdout must be the embedded document's exact bytes"
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout)
        .expect("config-surface stdout must parse as JSON");
    assert!(stderr(&out).is_empty(), "no stderr on success");
}

#[test]
fn contract_unknown_name_exits_one_and_names_every_valid_contract() {
    let out = run(&["contract", "nope"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "unknown name is a lookup failure"
    );
    let err = stderr(&out);
    for doc in zzop_mcp::embedded::CONTRACT_DOCS {
        assert!(
            err.contains(doc.name),
            "error must list {}: {err}",
            doc.name
        );
    }
    assert!(stdout(&out).is_empty(), "nothing on stdout for a failure");
}

/// A throwaway fixture dir (same pattern as the crate's other tests — no tempfile dep).
struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn endpoint_config_mode_runs_the_query_against_the_configs_trees() {
    // `endpoint <pattern> --config <path>` — the config-first mode the check_endpoint MCP tool has
    // always had (`configPath`), now reachable from the CLI like `cross --config`. The reply is the
    // shared query core's JSON with the honored config path stamped on top.
    let dir = TempDir::new("zzop-mcp-endpoint-config");
    dir.write(
        "src/api.ts",
        "export function load() { return fetch(\"/api/users\"); }\n",
    );
    dir.write(
        "zzop.config.jsonc",
        "{\n  // endpoint --config fixture\n  \"trees\": [{ \"root\": \".\", \"sourceId\": \"app\" }]\n}\n",
    );
    let config_path = dir.path().join("zzop.config.jsonc");

    let out = run(&[
        "endpoint",
        "users",
        "--config",
        config_path.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "expected exit 0, stderr: {}",
        stderr(&out)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout(&out)).expect("stdout must be the query JSON");
    assert!(
        v["verdict"].is_string(),
        "reply must carry the query core's verdict, got: {v}"
    );
    assert_eq!(
        v["counts"]["unprovidedConsumes"], 1,
        "the fixture's lone fetch must land in unprovidedConsumes, got: {v}"
    );
    assert_eq!(
        v["config"]
            .as_str()
            .map(|s| s.contains("zzop.config.jsonc")),
        Some(true),
        "the honored config path must be stamped on the reply, got: {v}"
    );
}

#[test]
fn analyze_dot_resolves_the_invocation_cwd_not_an_empty_root() {
    // The blind-test failure this pins: `.` used to survive verbatim into zzop-config's LEXICAL
    // normalization, which collapses all-CurDir paths to the EMPTY path — the engine then rejected
    // `root: ""` as a missing required field. Absolutized at the host boundary, `analyze .` from
    // inside a tree analyzes that tree.
    let dir = TempDir::new("zzop-mcp-analyze-dot");
    dir.write("src/api.ts", "export const a = 1;\n");

    let out = run_in(dir.path(), &["analyze", "."]);
    assert!(
        out.status.success(),
        "`zzop-mcp analyze .` must succeed, stderr: {}",
        stderr(&out)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout(&out)).expect("stdout must be the analyze summary JSON");
    assert!(
        v["fileCount"].as_u64().unwrap_or(0) > 0,
        "the cwd tree's files must be analyzed, got: {v}"
    );
}

#[test]
fn endpoint_relative_path_resolves_against_the_cwd_and_dir_names_its_source() {
    // Same boundary, endpoint's `path` mode: a relative tree argument resolves against the
    // invocation cwd, and the dir-name sourceId derives from the ABSOLUTIZED path (a relative
    // name used to be handed to zzop-config verbatim).
    let parent = TempDir::new("zzop-mcp-endpoint-relative");
    parent.write(
        "fe/src/api.ts",
        "export function load() { return fetch(\"/api/users\"); }\n",
    );

    let out = run_in(parent.path(), &["endpoint", "users", "fe"]);
    assert!(
        out.status.success(),
        "relative endpoint path must succeed, stderr: {}",
        stderr(&out)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout(&out)).expect("stdout must be the query JSON");
    assert_eq!(v["verdict"], "consumed-unprovided", "got: {v}");
    assert_eq!(
        v["matches"]["unprovidedConsumes"][0]["source"], "fe",
        "sourceId must derive from the absolutized path's dir name, got: {v}"
    );
}

/// `docs/NORMALIZED_AST.md`'s worked example (also the `example-envelope` MCP contract resource) —
/// copied to a real file here since the CLI subcommand reads a path, not inline JSON text.
const EXAMPLE_ENVELOPE: &str = include_str!("../../../examples/jsp-envelope.example.json");

#[test]
fn analyze_envelope_subcommand_runs_mode_a_over_a_file_and_prints_the_summary() {
    let dir = TempDir::new("zzop-mcp-analyze-envelope");
    dir.write("envelope.json", EXAMPLE_ENVELOPE);
    let path = dir.path().join("envelope.json");

    let out = run(&["analyze-envelope", path.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "expected exit 0, stderr: {}",
        stderr(&out)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout(&out)).expect("stdout must be the analyze summary JSON");
    assert!(v.get("findings").is_some(), "got: {v}");
    assert!(v.get("coverage").is_some(), "got: {v}");
    assert!(
        v.get("path").is_none(),
        "envelope mode has no filesystem root to echo, got: {v}"
    );
}

#[test]
fn analyze_envelope_subcommand_reports_an_unreadable_file_as_a_runtime_error() {
    let out = run(&["analyze-envelope", "/no/such/envelope.json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "unreadable file is a runtime error, not a usage error"
    );
    assert!(
        stderr(&out).contains("failed to read"),
        "got: {}",
        stderr(&out)
    );
}

#[test]
fn analyze_envelope_subcommand_requires_a_file_argument() {
    let out = run(&["analyze-envelope"]);
    assert_eq!(out.status.code(), Some(2), "usage errors exit 2");
    assert!(
        stderr(&out).contains("usage: zzop-mcp analyze-envelope"),
        "got: {}",
        stderr(&out)
    );
}

#[test]
fn fixed_arity_subcommands_reject_trailing_extra_args_instead_of_dropping_them() {
    // A silently-dropped trailing arg means the user believes it was analyzed — both fixed-arity
    // shapes must answer with a usage error (exit 2), like endpoint/contract already do.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_zzop-mcp"))
        .args(["analyze", "a", "b"])
        .output()
        .expect("spawn");
    assert_eq!(out.status.code(), Some(2), "analyze with 2 paths");
    assert!(String::from_utf8_lossy(&out.stderr).contains("one path"));

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_zzop-mcp"))
        .args(["analyze-envelope", "a.json", "b.json"])
        .output()
        .expect("spawn");
    assert_eq!(out.status.code(), Some(2), "analyze-envelope with 2 files");
    assert!(String::from_utf8_lossy(&out.stderr).contains("one file"));

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_zzop-mcp"))
        .args(["cross", "--config", "x.jsonc", "./extra"])
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(2),
        "cross --config with a trailing path"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("no extra paths"));
}
