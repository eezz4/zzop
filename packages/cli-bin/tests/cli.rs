//! Binary-level tests for the `zzop` CLI binary's argument dispatch (`src/main.rs`, package
//! `zzop-cli-bin`) — the thin layer the shared `zzop-summary` crate's own tests (`tests/host_dispatch.rs`,
//! handler-level) never exercise. Spawns the real `zzop` executable (`CARGO_BIN_EXE_zzop`, built by
//! cargo for integration tests), so exit codes and the stdout/stderr split are pinned exactly as a
//! shell sees them. The sibling `zzop-mcp` server binary's own non-serving surfaces (`version`,
//! unknown-arg) are smoke-tested separately in the `zzop-mcp` package (packages/mcp/tests/server_bin.rs).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zzop"))
        .args(args)
        .output()
        .expect("zzop binary should spawn")
}

/// Like `run`, but from a chosen working directory — the lane that pins relative-path arguments
/// (`analyze .`, `endpoint <pattern> <relative dir>`) resolving against the invocation cwd.
fn run_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zzop"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("zzop binary should spawn")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn version_subcommand_and_flag_print_the_server_version_and_exit_zero() {
    // Both spellings print the exact `server::version()` value (= `CARGO_PKG_VERSION`, the workspace
    // release SSOT) — the same string MCP `initialize` reports as serverInfo.version, so the CLI and the
    // protocol can never disagree. CI verifies the release tag matches it.
    for arg in ["version", "--version"] {
        let out = run(&[arg]);
        assert!(out.status.success(), "`zzop {arg}` must exit 0");
        assert_eq!(
            stdout(&out).trim(),
            format!("zzop {}", zzop_summary::version()),
            "`zzop {arg}` must print the server::version() value"
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
        assert!(out.status.success(), "`zzop {arg}` must exit 0");
        let text = stdout(&out);
        assert!(text.contains("usage:"), "`{arg}` got: {text}");
        assert!(text.contains("analyze"), "`{arg}` got: {text}");
        assert!(stderr(&out).is_empty(), "help is not an error");
    }
}

#[test]
fn analyze_unknown_flag_is_a_usage_error_never_a_path() {
    // The blind-test failure this pins: a dash-shaped argument used to be swallowed as a path and die
    // with "path does not exist: --nope" (exit 1). A dash-shaped argument in a path position is a
    // usage error, exit 2. (`--help` is NOT that case any more — see
    // `every_subcommand_answers_its_own_help_request_on_stdout_exit_zero`.)
    let out = run(&["analyze", "--nope"]);
    assert_eq!(out.status.code(), Some(2), "usage errors exit 2");
    let err = stderr(&out);
    assert!(err.contains("usage: zzop analyze"), "got: {err}");
    assert!(
        !err.contains("does not exist"),
        "must never be treated as a path: {err}"
    );
}

/// Every subcommand name the SHIPPED usage line offers, parsed out of `zzop help`'s own output — the
/// subject set for the help-lane pin below, so it can never fall behind a subcommand that shipped.
///
/// `main.rs`'s `USAGE` const is not reachable from an integration test (this is a binary crate), and
/// the point is to read what the binary actually prints anyway. The alternatives are separated by
/// `" | "` — with the spaces, because `[--domain <join|dep|risk|posture>]` spells a bare `|` inside one
/// of them — and each alternative leads with its subcommand name. The parse is deliberately dumb; the
/// caller asserts the result is not a stub.
fn subcommands_named_by_the_usage_line() -> Vec<String> {
    let help = stdout(&run(&["help"]));
    let body = help
        .split_once("zzop <")
        .and_then(|(_, rest)| rest.split_once("> ("))
        .map(|(body, _)| body.to_string())
        .unwrap_or_else(|| {
            panic!("`zzop help` no longer prints a `zzop <...>` usage line: {help}")
        });
    let mut subs: Vec<String> = body
        .split(" | ")
        .filter_map(|alt| alt.split_whitespace().next())
        .filter(|name| {
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        .map(str::to_string)
        .collect();
    subs.sort();
    subs.dedup();
    assert!(
        subs.len() >= 10,
        "the usage-line parse found {subs:?} — it has stopped matching, so a test built on it would \
         vouch for nothing"
    );
    subs
}

/// Seals item ⑤ of the CLI restoration: a help REQUEST is answered, not rejected. Every subcommand's
/// own `-h`/`--help` prints THAT subcommand's line to stdout and exits 0 — before this, the request fell
/// into the dash-shaped-argument guard and left with exit 2 on stderr, handing an error to the one
/// caller who asked for help.
#[test]
fn every_subcommand_answers_its_own_help_request_on_stdout_exit_zero() {
    // Every subcommand the usage line names, READ OUT OF THE BINARY'S OWN usage line and checked
    // through the real binary. It used to be a hand-typed list of 14, which is exactly the shape of
    // blindness this test exists to prevent one level down: `zzop file` shipped and never joined the
    // list, so the one surface added after the list was written was the one surface never checked.
    let subs = subcommands_named_by_the_usage_line();
    for sub in &subs {
        let sub = sub.as_str();
        for flag in ["-h", "--help"] {
            let out = run(&[sub, flag]);
            assert_eq!(
                out.status.code(),
                Some(0),
                "`zzop {sub} {flag}` must exit 0 (a help request is not an error)"
            );
            assert!(
                stderr(&out).is_empty(),
                "`zzop {sub} {flag}` must print nothing to stderr: {}",
                stderr(&out)
            );
            let text = stdout(&out);
            assert!(
                text.starts_with("usage: zzop ") && text.contains(sub),
                "`zzop {sub} {flag}` must print that subcommand's own usage line: {text}"
            );
        }
    }
    // An UNKNOWN subcommand keeps the usage-error lane — the help gate must not swallow it.
    let out = run(&["nope", "--help"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "unknown subcommand still exits 2"
    );
}

/// The findings-view knobs (item ②) reach the CLI through the SHARED filters, with the shared
/// validation vocabulary: a bad value is an argument-shape error (exit 2), never a silent no-op.
#[test]
fn findings_filter_knobs_are_wired_and_reject_bad_values_as_usage_errors() {
    let dir = TempDir::new("zzop-cli-filters");
    dir.write("a.ts", "export const a = 1;\n");
    init_config(dir.path());
    let path = dir.path().display().to_string();

    // `--limit 0` is legal and means "counts only" — the same contract the MCP `limit` argument has.
    let out = run(&["analyze", &path, "--limit", "0"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("analyze prints JSON");
    assert!(
        v["findings"]["shown"]
            .as_array()
            .expect("shown array")
            .is_empty(),
        "limit 0 lists nothing: {}",
        stdout(&out)
    );

    for bad in [
        vec!["analyze", &path, "--severity", "nope"],
        vec!["analyze", &path, "--limit", "99999"],
        vec!["analyze", &path, "--limit", "abc"],
        vec!["analyze", &path, "--rule"],
    ] {
        let out = run(&bad);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{bad:?} must be a usage error: {}",
            stdout(&out)
        );
        assert!(
            stderr(&out).contains("usage: zzop analyze"),
            "{bad:?}: {}",
            stderr(&out)
        );
    }
}

/// Item ③a: `analyze --config <file>` analyzes the ONE tree a config at any location names — the mode
/// that simply did not exist (the fixed-arity parser rejected every dash-shaped argument), leaving a
/// config outside the tree root unreachable from a terminal.
#[test]
fn analyze_config_mode_analyzes_the_tree_the_config_names() {
    let dir = TempDir::new("zzop-cli-analyze-config");
    dir.write("app/a.ts", "export const a = 1;\n");
    std::fs::create_dir_all(dir.path().join("ci")).unwrap();
    dir.write("ci/zzop.config.jsonc", "{ \"roots\": [\"../app\"] }\n");
    let cp = dir
        .path()
        .join("ci")
        .join("zzop.config.jsonc")
        .display()
        .to_string();

    let out = run(&["analyze", "--config", &cp]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("analyze prints JSON");
    assert_eq!(v["config"].as_str(), Some(cp.as_str()));
    assert!(
        v["path"].as_str().expect("path echo").ends_with("app"),
        "the echoed path is the analyzed TREE root, not the config file: {}",
        stdout(&out)
    );

    // A trailing path after `--config` would be silently DROPPED — the same never-silent guard every
    // multi-tree sibling carries.
    let out = run(&["analyze", "--config", &cp, "./extra"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("no extra paths"), "{}", stderr(&out));
}

/// Item ④: `version` stays a bare one-token line (scripts parse it); `--verbose` adds the parser
/// fingerprints. The two forms report ONE version — the verbose line must start with the bare one.
#[test]
fn version_verbose_adds_parser_fingerprints_without_moving_the_bare_line() {
    let bare = run(&["version"]);
    assert_eq!(bare.status.code(), Some(0));
    let verbose = run(&["version", "--verbose"]);
    assert_eq!(verbose.status.code(), Some(0), "{}", stderr(&verbose));
    let text = stdout(&verbose);
    assert!(
        text.starts_with(&format!("zzop/{}", zzop_summary::version())),
        "the verbose form leads with the same version: {text}"
    );
    assert!(
        text.contains("zzop-parser-typescript="),
        "the verbose form carries parser fingerprints: {text}"
    );
    assert_ne!(stdout(&bare), text, "the two forms differ");
    // Anything else after `version` is an argument-shape mistake, never a silently ignored option.
    let bad = run(&["version", "--nope"]);
    assert_eq!(bad.status.code(), Some(2));
}

#[test]
fn endpoint_flag_like_pattern_is_a_usage_error_never_a_pattern() {
    let out = run(&["endpoint", "-x", "a", "b"]);
    assert_eq!(out.status.code(), Some(2), "usage errors exit 2");
    assert!(
        stderr(&out).contains("usage: zzop endpoint"),
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
        stderr(&out).contains("usage: zzop endpoint <pattern> --config"),
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
    // The terminal lane to the embedded authoring contracts: `contract` with no name lists every one
    // (name + description + mime, human-readable lines) — a terminal user must never have to
    // reverse-engineer the config surface from error messages while the docs sit inside the binary.
    let out = run(&["contract"]);
    assert!(out.status.success(), "`zzop contract` must exit 0");
    let text = stdout(&out);
    for doc in zzop_summary::contracts::CONTRACT_DOCS {
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
        "`zzop contract config-surface` must exit 0"
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
    for doc in zzop_summary::contracts::CONTRACT_DOCS {
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

/// Puts a real starter config in `dir`, the way a user starts: by running `zzop init` in it.
///
/// Every analysis lane requires a config as of 2026-07-27, so a fixture tree needs one before it can be
/// analyzed at all. Written by the binary under test rather than by a literal here, so these fixtures can
/// never drift from the document `init` actually ships — and so the tests below stay about their own
/// subject instead of re-stating the template.
fn init_config(dir: &Path) {
    let out = run_in(dir, &["init"]);
    assert!(
        out.status.success(),
        "`zzop init` must seed the fixture, stderr: {}",
        stderr(&out)
    );
}

#[test]
fn endpoint_config_mode_runs_the_query_against_the_configs_trees() {
    // `endpoint <pattern> --config <path>` — the config-first mode the check_endpoint MCP tool has
    // always had (`configPath`), now reachable from the CLI like `cross --config`. The reply is the
    // shared query core's JSON with the honored config path stamped on top.
    let dir = TempDir::new("zzop-endpoint-config");
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
    let dir = TempDir::new("zzop-analyze-dot");
    dir.write("src/api.ts", "export const a = 1;\n");
    init_config(dir.path());

    let out = run_in(dir.path(), &["analyze", "."]);
    assert!(
        out.status.success(),
        "`zzop analyze .` must succeed, stderr: {}",
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
    let parent = TempDir::new("zzop-endpoint-relative");
    parent.write(
        "fe/src/api.ts",
        "export function load() { return fetch(\"/api/users\"); }\n",
    );

    init_config(&parent.path().join("fe"));

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
const EXAMPLE_ENVELOPE: &str = include_str!("../../../docs/contracts/example-envelope.json");

#[test]
fn analyze_envelope_subcommand_runs_mode_a_over_a_file_and_prints_the_summary() {
    let dir = TempDir::new("zzop-analyze-envelope");
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

/// `--profile-rules` on the envelope lane — this exact invocation used to be an exit-2 refusal by
/// name ("Mode A analysis produces no rule timings"), back when `analyze_envelope` ran its packs
/// outside the engine's timing accumulator. The wiring landed, so the refusal is gone and the flag
/// produces a real report: exit 0 with a non-empty `ruleTimings` object in the summary.
#[test]
fn analyze_envelope_subcommand_takes_profile_rules_and_reports_timings() {
    let dir = TempDir::new("zzop-analyze-envelope-profile");
    dir.write("envelope.json", EXAMPLE_ENVELOPE);
    let path = dir.path().join("envelope.json");

    let out = run(&[
        "analyze-envelope",
        path.to_str().unwrap(),
        "--profile-rules",
    ]);
    assert!(
        out.status.success(),
        "the old exit-2 refusal must be gone, stderr: {}",
        stderr(&out)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout(&out)).expect("stdout must be the analyze summary JSON");
    let rules = v["ruleTimings"]["rules"]
        .as_array()
        .unwrap_or_else(|| panic!("a profiled Mode A run must carry ruleTimings.rules, got: {v}"));
    assert!(!rules.is_empty(), "got: {v}");

    // The unprofiled invocation stays byte-identical to the pre-knob lane: no key at all.
    let off = run(&["analyze-envelope", path.to_str().unwrap()]);
    assert!(off.status.success(), "stderr: {}", stderr(&off));
    let off: serde_json::Value =
        serde_json::from_str(&stdout(&off)).expect("stdout must be the analyze summary JSON");
    assert!(off.get("ruleTimings").is_none(), "got: {off}");
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

// ---------------------------------------------------------------------------------------------
// `zzop explain <rule-id>` — the read-only lookup over the DSL rule data compiled into this binary
// (`zzop_summary::explain`). The ambiguous-bare-id lane (a bare id shared by two packs) is NOT pinned
// here: `derived_suppress_markers_are_globally_unique`
// (`crates/engine/tests/rule_contracts/markers.rs`) machine-enforces that every shipped rule id is
// globally unique today, so real bundled data has no bare-id collision to spawn the binary against —
// that lane is instead pinned against a fabricated pack pair in `crates/facade/src/explain/tests.rs`
// (`a_bare_id_shared_by_two_packs_is_ambiguous_and_lists_both_full_ids`).
// ---------------------------------------------------------------------------------------------

#[test]
fn explain_full_id_prints_the_rule_and_exits_zero() {
    let out = run(&["explain", "sql/nplus1"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("id: sql/nplus1"), "got: {text}");
    assert!(text.contains("pack: sql"), "got: {text}");
    assert!(text.contains("severity: warning"), "got: {text}");
    assert!(
        text.contains("suppress marker: zzop-nplus1-ok"),
        "got: {text}"
    );
    assert!(text.contains("matcher: method-scan"), "got: {text}");
    // The exclusion lines are the ones method-scan actually HAS: its `absent` veto (this rule declares
    // none) and `file_exclude_pattern` (this rule DOES carry one — the expanded `${test-paths-stories}`
    // fragment). The pre-fix render printed a single blanket `exclude_pattern: no` here, which was a
    // false report of the second one; the negative assert keeps that spelling from coming back.
    assert!(text.contains("absent: no"), "got: {text}");
    assert!(
        text.contains("file_exclude_pattern: (?i)("),
        "method-scan rule with a real file_exclude_pattern must print it: {text}"
    );
    assert!(
        !text.contains("\nexclude_pattern:"),
        "method-scan has no `exclude_pattern` field, so no such line may be printed: {text}"
    );
    assert!(stderr(&out).is_empty(), "no stderr on success");
}

/// The io-scan lane of the same contract, and the reason it was worth fixing: `http/dev-path-no-guard-hint`
/// carries BOTH an `anchor_exclude_pattern` and a `file_exclude_pattern`, and the pre-fix render reported
/// `exclude_pattern: no` for it — the tool lying about its own vetoes, which sends the reader off to file
/// a false-negative report or to build a carve-out that already exists.
#[test]
fn explain_reports_an_io_scan_rules_real_exclusion_fields() {
    let out = run(&["explain", "http/dev-path-no-guard-hint"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("matcher: io-scan"), "got: {text}");
    assert!(
        text.contains("anchor_exclude_pattern: (?i),"),
        "io-scan `anchor_exclude_pattern` must be printed verbatim: {text}"
    );
    assert!(
        text.contains("file_exclude_pattern: (?i)("),
        "io-scan `file_exclude_pattern` must be printed verbatim: {text}"
    );
    // io-scan honors a line-comment-NEUTRAL marker (`//` or `#`), unlike the `//`-only line/method-scan
    // passes — a difference the flat `suppress marker: <id>-ok` line used to hide from Python readers.
    assert!(
        text.contains(
            "suppress marker: zzop-dev-path-no-guard-hint-ok (in a `//` or `#` line comment"
        ),
        "got: {text}"
    );
}

/// The ATTRIBUTE-gate lane of the same contract. It matters more than the regex lanes: with
/// `require_attr_declared` set and nothing declaring that key, the rule does not run AT ALL, so a reader
/// who cannot see the field reads the resulting silence as a false negative — the exact misreading this
/// section exists to prevent. `reliability/env-outside-config` is the shipped rule that lives on all
/// three fields; it went silent-by-default in v0.24.0, which is when a reader most needs to be told why.
///
/// The rule moved from `line-scan` to `call-scan` in 2026-08-03's call-site wave, and the matcher kind is
/// asserted here rather than left loose precisely because the three gates are declared SEPARATELY on each
/// matcher struct: a migration that carried the rule across but dropped a gate would otherwise turn this
/// rule permanently silent-or-permanently-loud with nothing red. `docs/rules/dsl-reference.md`'s
/// attribute-gate table is where the two matchers' identical semantics are stated.
#[test]
fn explain_reports_a_call_scan_rules_attribute_gates() {
    let out = run(&["explain", "reliability/env-outside-config"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("matcher: call-scan"), "got: {text}");
    for field in [
        "attr_absent: env-config-module",
        "require_attr_declared: env-config-module",
    ] {
        assert!(
            text.contains(field),
            "the gate that decides whether this rule runs must be printed (`{field}`): {text}"
        );
    }
    // Printed even when unset, same as the regex lanes: the reader learns which gates the matcher offers.
    assert!(text.contains("attr_present: no"), "got: {text}");
}

/// The POSITIVE half, end to end, and the pin that other documents now rest on. Nine verbatim copies of
/// rule `file_pattern` regexes were deleted from rule messages, `docs/rules/catalog.md` and
/// `site/rules.html` on 2026-08-01 because `zzop explain` is the canonical answer instead — a trade that
/// only holds while the binary actually prints it, which it did not until the scope block shipped
/// (`crates/facade/src/explain/scope.rs`).
///
/// DERIVED, never copied: the expected regex is read out of the same bundled pack data the binary
/// renders from, so this pin cannot itself become a tenth stale copy. It covers one rule per matcher
/// KIND, chosen from what actually ships — a new matcher kind is pinned the moment its first rule lands,
/// with no table here to update. The per-field completeness sweep (every field of every matcher struct,
/// including the kinds no pack ships) lives in `crates/facade/src/explain/field_coverage_tests.rs`; this
/// checks the value survives the real process boundary.
#[test]
fn explain_prints_each_shipped_matcher_kinds_own_file_pattern() {
    let mut per_kind: std::collections::BTreeMap<&'static str, (String, String)> =
        std::collections::BTreeMap::new();
    for (rel_path, source) in zzop_config::BUNDLED_PACK_SOURCES {
        let pack = zzop_core::parse_dsl_pack(source)
            .unwrap_or_else(|e| panic!("bundled pack {rel_path} must parse: {e}"));
        for rule in &pack.rules {
            let (kind, file_pattern) = match &rule.matcher {
                zzop_core::Matcher::LineScan(m) => ("line-scan", &m.file_pattern),
                zzop_core::Matcher::MethodScan(m) => ("method-scan", &m.file_pattern),
                zzop_core::Matcher::SymbolScan(m) => ("symbol-scan", &m.file_pattern),
                zzop_core::Matcher::IoScan(m) => ("io-scan", &m.file_pattern),
                zzop_core::Matcher::CallScan(m) => ("call-scan", &m.file_pattern),
                zzop_core::Matcher::LiteralScan(m) => ("literal-scan", &m.file_pattern),
            };
            per_kind
                .entry(kind)
                .or_insert_with(|| (format!("{}/{}", pack.id, rule.id), file_pattern.clone()));
        }
    }
    assert!(
        !per_kind.is_empty(),
        "no bundled DSL rule found at all — this test would then vouch for nothing"
    );

    for (kind, (full_id, file_pattern)) in &per_kind {
        let out = run(&["explain", full_id]);
        assert!(out.status.success(), "{full_id}: stderr: {}", stderr(&out));
        let text = stdout(&out);
        assert!(
            text.contains(&format!("matcher: {kind}")),
            "{full_id}: expected matcher kind {kind}: {text}"
        );
        assert!(
            text.contains(&format!("\nfile_pattern: {file_pattern}\n")),
            "{full_id}: `explain` must print this rule's own file_pattern verbatim — the copies in \
             catalog.md/site/rules.html were deleted in favour of exactly this line: {text}"
        );
    }
}

/// The rest of the positive scope for the two kinds most rules use, on a shipped rule that exercises
/// them: `sql/nplus1` is the method-scan whose `require_file` pre-skip and `trigger_in_loop` structural
/// gate both narrow what it looks at, and printing `patterns` without them would overclaim the rule as
/// plain co-occurrence. Values are asserted by SHAPE (field present, non-`no`) rather than copied, for
/// the same no-tenth-copy reason as the test above.
#[test]
fn explain_prints_the_pre_skips_and_structural_gates_that_narrow_a_rule() {
    let out = run(&["explain", "sql/nplus1"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    for field in [
        "require_file",
        "require_file_all",
        "patterns",
        "trigger",
        "trigger_in_loop",
        "after",
        "after_in_same_function",
        "skip_comment_lines",
        "strip_string_literals",
    ] {
        assert!(
            text.contains(&format!("\n{field}: ")),
            "method-scan scope field `{field}` must be reported: {text}"
        );
    }
    // `require_file` is a whole-FILE pre-skip: a file whose text misses it is never scanned, so a
    // reader answering "would this rule look at my file?" needs its real value, not just its name.
    assert!(
        !text.contains("\nrequire_file: no\n"),
        "sql/nplus1 declares a require_file pre-skip — printing `no` would be a false report: {text}"
    );
    assert!(
        text.contains("\ntrigger_in_loop: yes\n"),
        "sql/nplus1's loop-containment gate must be visible, or `patterns` reads as plain \
         co-occurrence: {text}"
    );
    // Not scope and labelled as such — the one matcher field that cannot change whether a rule fires.
    assert!(
        text.contains("\nsnippet_max: 160 (snippet truncation only"),
        "got: {text}"
    );
}

/// The reduced NATIVE lane must not grow a scope block. A native analysis has no `file_pattern` at all
/// (it is compiled Rust, not DSL pack data), so a line claiming one would be pure fabrication — the
/// failure this whole change exists to avoid, in the opposite direction.
#[test]
fn explain_never_prints_a_scope_field_for_a_native_analysis_id() {
    for query in ["circular", "schema/god-model", "god-model"] {
        let out = run(&["explain", query]);
        assert_eq!(out.status.code(), Some(1), "{query} renders no DSL data");
        let err = stderr(&out);
        for field in ["file_pattern", "require_file", "line_pattern", "patterns:"] {
            assert!(
                !err.contains(field),
                "{query} is native and has no `{field}` — naming one would fabricate it: {err}"
            );
        }
        assert!(
            stdout(&out).is_empty(),
            "{query}: nothing on stdout for a failure"
        );
    }
}

#[test]
fn explain_bare_id_resolves_when_unambiguous() {
    // Every shipped rule id is globally unique (see this section's header comment), so any bare id
    // resolves to the exact same rule its full form does.
    let out = run(&["explain", "nplus1"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("id: sql/nplus1"),
        "got: {}",
        stdout(&out)
    );
}

#[test]
fn explain_unknown_id_exits_one_and_points_at_the_catalog() {
    let out = run(&["explain", "no-such-rule-anywhere"]);
    assert_eq!(out.status.code(), Some(1), "unknown id is a lookup failure");
    let err = stderr(&out);
    assert!(err.contains("unknown rule id"), "got: {err}");
    assert!(err.contains("rule-catalog"), "got: {err}");
    assert!(stdout(&out).is_empty(), "nothing on stdout for a failure");
}

#[test]
fn explain_native_analysis_id_exits_one_and_names_it_native_not_missing() {
    // `circular` is a native analysis (`rules/native/rules-graph`), never a bundled DSL pack — the
    // message must say so, not imply the id does not exist.
    let out = run(&["explain", "circular"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "native ids are a lookup failure here too"
    );
    let err = stderr(&out);
    assert!(err.contains("native analysis id"), "got: {err}");
    assert!(err.contains("rule-catalog"), "got: {err}");
}

/// The promoted `schema/*` ids, end to end through the real binary. `schema/god-model` is the exact
/// string a schema finding puts in `ruleId`; it used to be an unregistered label, so this lookup answered
/// "unknown rule id" about zzop's own output. It is a registered analysis now, and the BARE tail a reader
/// might type instead is answered by naming the full id `disabledRules` actually matches.
#[test]
fn explain_answers_a_promoted_schema_id_in_both_forms() {
    let full = run(&["explain", "schema/god-model"]);
    assert_eq!(full.status.code(), Some(1), "native ids render no DSL data");
    let err = stderr(&full);
    assert!(err.contains("native analysis id"), "got: {err}");
    assert!(!err.contains("unknown rule id"), "got: {err}");

    let bare = run(&["explain", "god-model"]);
    assert_eq!(bare.status.code(), Some(1));
    let err = stderr(&bare);
    assert!(err.contains("schema/god-model"), "got: {err}");
    assert!(!err.contains("unknown rule id"), "got: {err}");
}

/// The G14 class, one namespace over from the `schema/<label>` lane: strings zzop prints under a field
/// literally named `id`/`group` in EVERY analyze reply — a coverage-disclosure class and its taxonomy
/// group (`disclosure[]`) and a recommendation id (`architecture.topRecommendation.id`) — used to come
/// back as "unknown rule id", the tool denying its own output. They are still lookup FAILURES (there is
/// no DSL rule to render), but guided ones that name what the id actually is. The exhaustive sweep over
/// the live registry lives in `crates/facade/src/explain/output_ids.rs`; this pins the wire behavior.
#[test]
fn explain_output_ids_exit_one_and_name_what_they_actually_are() {
    for (query, expected) in [
        ("stale-cache", "coverage-DISCLOSURE class id"),
        ("trust-calibration", "coverage-disclosure GROUP"),
        ("hot-churn", "RECOMMENDATION id"),
    ] {
        let out = run(&["explain", query]);
        assert_eq!(
            out.status.code(),
            Some(1),
            "{query} carries no DSL rule data"
        );
        let err = stderr(&out);
        assert!(err.contains(expected), "{query}: got: {err}");
        assert!(
            !err.contains("unknown rule id"),
            "{query} must never be denied — it is zzop's own output: {err}"
        );
        assert!(
            stdout(&out).is_empty(),
            "{query}: nothing on stdout for a failure"
        );
    }
}

#[test]
fn explain_usage_errors_exit_two() {
    let missing = run(&["explain"]);
    assert_eq!(
        missing.status.code(),
        Some(2),
        "missing id is a usage error"
    );
    assert!(
        stderr(&missing).contains("usage: zzop explain"),
        "got: {}",
        stderr(&missing)
    );

    let extra = run(&["explain", "sql/nplus1", "extra"]);
    assert_eq!(extra.status.code(), Some(2), "extra arg is a usage error");
    assert!(stderr(&extra).contains("one id"), "got: {}", stderr(&extra));

    let flag_like = run(&["explain", "--nope"]);
    assert_eq!(
        flag_like.status.code(),
        Some(2),
        "flag-like id is a usage error"
    );
    assert!(
        !stderr(&flag_like).contains("unknown rule id"),
        "a dash-shaped arg must never be treated as a query: {}",
        stderr(&flag_like)
    );
}

/// Every rule id shipped in every bundled DSL pack must be explainable (exit 0) — no shipped rule can
/// be un-explainable. Loads the packs the exact same way `zzop_summary::explain` does
/// (`zzop_core::parse_dsl_pack` over `zzop_config::BUNDLED_PACK_SOURCES`), never a hand-copied id list,
/// so this can't drift from what actually ships.
#[test]
fn every_bundled_dsl_rule_id_is_explainable() {
    let mut offenders = Vec::new();
    for (rel_path, source) in zzop_config::BUNDLED_PACK_SOURCES {
        let pack = zzop_core::parse_dsl_pack(source)
            .unwrap_or_else(|e| panic!("bundled pack {rel_path} must parse: {e}"));
        for rule in &pack.rules {
            let full_id = format!("{}/{}", pack.id, rule.id);
            let out = run(&["explain", &full_id]);
            if !out.status.success() {
                offenders.push(format!(
                    "{full_id}: exit {:?}, stderr: {}",
                    out.status.code(),
                    stderr(&out)
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "shipped rule ids that `zzop explain` cannot explain: {offenders:#?}"
    );
}

#[test]
fn analyze_envelope_subcommand_requires_a_file_argument() {
    let out = run(&["analyze-envelope"]);
    assert_eq!(out.status.code(), Some(2), "usage errors exit 2");
    assert!(
        stderr(&out).contains("usage: zzop analyze-envelope"),
        "got: {}",
        stderr(&out)
    );
}

#[test]
fn fixed_arity_subcommands_reject_trailing_extra_args_instead_of_dropping_them() {
    // A silently-dropped trailing arg means the user believes it was analyzed — both fixed-arity
    // shapes must answer with a usage error (exit 2), like endpoint/contract already do.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_zzop"))
        .args(["analyze", "a", "b"])
        .output()
        .expect("spawn");
    assert_eq!(out.status.code(), Some(2), "analyze with 2 paths");
    assert!(String::from_utf8_lossy(&out.stderr).contains("one path"));

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_zzop"))
        .args(["analyze-envelope", "a.json", "b.json"])
        .output()
        .expect("spawn");
    assert_eq!(out.status.code(), Some(2), "analyze-envelope with 2 files");
    assert!(String::from_utf8_lossy(&out.stderr).contains("one file"));

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_zzop"))
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

/// Builds a two-tree fixture (a frontend that calls `/api/users`, a backend that does not provide it)
/// plus a `zzop.config.jsonc` naming both trees, and returns the config path. `cache_dir` opts the
/// run into the on-disk cache, which the cold/warm pin below needs.
fn manifest_fixture(dir: &TempDir, cache_dir: Option<&str>) -> PathBuf {
    dir.write(
        "fe/src/api.ts",
        "export function load() { return fetch(\"/api/users\"); }\n",
    );
    dir.write("be/src/db.ts", "export const table = 'users';\n");
    let cache = cache_dir
        .map(|c| format!(",\n  \"cacheDir\": \"{c}\""))
        .unwrap_or_default();
    dir.write(
        "zzop.config.jsonc",
        &format!(
            "{{\n  // manifest fixture\n  \"trees\": [\
             {{ \"root\": \"./fe\", \"sourceId\": \"fe\" }}, \
             {{ \"root\": \"./be\", \"sourceId\": \"be\" }}]{cache}\n}}\n"
        ),
    );
    dir.path().join("zzop.config.jsonc")
}

#[test]
fn manifest_emits_identity_rows_and_never_a_path_or_a_line() {
    // The identity contract, end to end through the real binary: a manifest names WHAT (kind/key/
    // source), never WHERE. A file path or a line number here would make one refactor drown the
    // signal `diff` exists to surface — and an absolute `root` would make a laptop's manifest
    // un-diffable against CI's.
    let dir = TempDir::new("zzop-manifest");
    let config = manifest_fixture(&dir, None);
    let out = run(&["manifest", "--config", config.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let v: serde_json::Value = serde_json::from_str(&text).expect("manifest must be JSON");

    assert!(v["tool"].as_str().unwrap().starts_with("zzop/"), "{v}");
    let ids: Vec<&str> = v["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["sourceId"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["be", "fe"], "sorted by sourceId, not tree order");
    assert_eq!(
        v["buckets"],
        serde_json::json!([{
            "bucket": "unprovidedConsumes", "kind": "http",
            "key": "GET /api/users", "source": "fe",
        }]),
        "the fe's unmatched call is a bucket MEMBERSHIP row: {v}"
    );
    for banned in ["\"file\"", "\"line\"", "\"root\"", "src/api.ts"] {
        assert!(!text.contains(banned), "manifest carries {banned}: {text}");
    }
}

#[test]
fn a_manifest_is_byte_identical_cold_and_warm() {
    // The cache must never change an ANSWER, only the time to get one — so the same tree analyzed
    // cold and then warm must project to the same bytes. This pin is the cheap place that contract
    // gets checked end-to-end: if it ever fails it is a cache bug, not a manifest bug, and the
    // manifest is the surface where such a bug would read as "the other team changed something".
    let dir = TempDir::new("zzop-manifest-cache");
    let config = manifest_fixture(&dir, Some("./.zzop-cache"));
    let cold = run(&["manifest", "--config", config.to_str().unwrap()]);
    assert!(cold.status.success(), "stderr: {}", stderr(&cold));
    let warm = run(&["manifest", "--config", config.to_str().unwrap()]);
    assert!(warm.status.success(), "stderr: {}", stderr(&warm));
    assert_eq!(stdout(&cold), stdout(&warm), "cache changed the manifest");
}

#[test]
fn manifest_argument_shapes_are_usage_errors_exactly_like_cross() {
    // One shared argv parser backs `cross` and `manifest`, so the silent-narrowing traps close
    // identically on both: a single path is not a join, and a path after `--config` would be DROPPED.
    let out = run(&["manifest", "./only-one"]);
    assert_eq!(out.status.code(), Some(2), "one path is not a join");
    assert!(
        stderr(&out).contains("usage: zzop manifest"),
        "{}",
        stderr(&out)
    );
    let out = run(&["manifest", "--config", "x.jsonc", "./extra"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("no extra paths"), "{}", stderr(&out));
    let out = run(&["manifest", "--nope"]);
    assert_eq!(out.status.code(), Some(2), "a flag is never a path");
}

#[test]
fn diff_reports_nothing_for_an_identical_pair_and_refuses_a_cross_build_one() {
    // Honesty gate 1 through the real binary, BOTH directions: a same-build pair diffs (and a pure
    // no-op is empty), a cross-build pair is REFUSED with the escape hatch named, and forcing it
    // discloses rather than going quiet. Exit codes are part of the contract a CI script reads.
    let dir = TempDir::new("zzop-diff");
    let config = manifest_fixture(&dir, None);
    let out = run(&["manifest", "--config", config.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    dir.write("a.json", &stdout(&out));
    let mut other: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    dir.write("same.json", &stdout(&out));
    other["tool"] = serde_json::json!("zzop/0.0.1-other zzop-parser-typescript=OLD");
    dir.write("old.json", &other.to_string());
    let p = |name: &str| dir.path().join(name).to_str().unwrap().to_string();

    let same = run(&["diff", &p("a.json"), &p("same.json")]);
    assert!(same.status.success(), "stderr: {}", stderr(&same));
    let d: serde_json::Value = serde_json::from_str(&stdout(&same)).unwrap();
    assert_eq!(d["transitions"], serde_json::json!([]));
    assert_eq!(d["buckets"]["removed"], serde_json::json!([]));

    let refused = run(&["diff", &p("a.json"), &p("old.json")]);
    assert_eq!(
        refused.status.code(),
        Some(1),
        "a refusal is a runtime failure"
    );
    assert!(
        stderr(&refused).contains("--allow-tool-drift"),
        "{}",
        stderr(&refused)
    );
    assert!(stdout(&refused).is_empty(), "a refusal prints no diff");

    let forced = run(&["diff", &p("a.json"), &p("old.json"), "--allow-tool-drift"]);
    assert!(forced.status.success());
    let d: serde_json::Value = serde_json::from_str(&stdout(&forced)).unwrap();
    assert!(
        d["toolDrift"]["b"]
            .as_str()
            .unwrap()
            .contains("0.0.1-other"),
        "{d}"
    );
}

#[test]
fn diff_argument_shapes_are_usage_errors() {
    let out = run(&["diff", "only-one.json"]);
    assert_eq!(out.status.code(), Some(2), "diff needs two manifests");
    assert!(
        stderr(&out).contains("two manifest files"),
        "{}",
        stderr(&out)
    );
    let out = run(&["diff", "-x", "b.json"]);
    assert_eq!(out.status.code(), Some(2), "a flag is never a filename");
    // A missing file is a RUNTIME failure (exit 1), not an arg-shape one — the same two-lane split
    // every file-taking subcommand carries.
    let out = run(&["diff", "nope-a.json", "nope-b.json"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("failed to read"), "{}", stderr(&out));
}

// DELETED 2026-07-28: `help_and_usage_both_name_the_two_new_subcommands`. It claimed to pin that "a new
// subcommand actually reached BOTH surfaces" while iterating a four-name literal — and its own name
// still said "two". Every part of that claim is already made, derivatively and over the FULL set, by
// `src/cli/help/tests.rs` (every dispatched subcommand has an elaboration row; the usage line names
// every described one) plus the two binary-level lane pins here
// (`top_level_help_prints_the_usage_line_to_stdout_and_exits_zero` for help→stdout,
// `no_args_usage_error_names_every_subcommand_including_version` for bare-invocation usage→stderr).
// What it uniquely covered was four names out of fifteen, which is worse than nothing: it read as
// coverage. Subtraction bias — deleted rather than converted, because converting it would have
// produced a third spelling of a contract two derived tests already hold.

#[test]
fn facts_emits_the_uncapped_post_assembly_substrate_for_one_tree() {
    // The custom-rule extension point's EMIT half, end to end through the real binary. Unlike
    // `cross`/`manifest`, ONE path is a legal invocation: the join runs over a single tree (intra-tree
    // edges included), so a rule author dumping facts for one repo is not forced to invent a second.
    let dir = TempDir::new("zzop-facts-one");
    dir.write(
        "src/api.ts",
        "export function load() { return fetch(\"/api/users\"); }\n",
    );
    init_config(dir.path());
    let out = run(&["facts", dir.path().to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("facts must be JSON");

    // §0: every channel is present, so a zero can never be read as "did not run".
    for key in [
        "tool",
        "config",
        "configWarnings",
        "trees",
        "crossLayer",
        "warnings",
        "disclosure",
    ] {
        assert!(v.get(key).is_some(), "facts must always carry `{key}`: {v}");
    }
    assert!(v["tool"].as_str().unwrap().starts_with("zzop/"), "{v}");
    let tree = &v["trees"][0];
    assert!(tree["coverage"]["files"].as_u64().unwrap() >= 1, "{tree}");
    // The whole IR, with the file/line detail `zzop manifest` deliberately strips — a rule program has
    // to be able to report a location.
    assert!(tree["commonIr"]["symbols"].is_array(), "{tree}");
    assert!(tree["commonIr"]["dep"].is_object(), "{tree}");
    assert!(tree["commonIr"]["loc"].is_object(), "{tree}");
    assert!(tree["commonIr"]["io"]["consumes"].is_array(), "{tree}");
    let consume = &tree["commonIr"]["io"]["consumes"][0];
    assert_eq!(consume["key"], "GET /api/users", "{tree}");
    assert_eq!(consume["file"], "src/api.ts", "{tree}");
    // Facts, not verdicts — zzop's own findings never ride this surface.
    assert!(v.get("findings").is_none(), "{v}");
    assert!(v.get("crossLayerFindings").is_none(), "{v}");
    assert!(tree.get("findings").is_none(), "{tree}");
}

#[test]
fn coverage_aggregates_dispatch_by_extension_and_declares_recall_unmeasured() {
    // The aggregate-visibility lane, end to end through the real binary: one tree holding a structural
    // file (.ts) and a lexical-only one (.md) must land in different dispatch rows, and the reply must
    // carry the two honesty features the 2026-07-31 ruling is about — the unmeasured axis as a FIELD,
    // and no single-score key anywhere in the schema.
    let dir = TempDir::new("zzop-coverage-one");
    dir.write("src/api.ts", "export function load() { return 1; }\n");
    dir.write("README.md", "# hello\n");
    init_config(dir.path());
    let out = run(&["coverage", dir.path().to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("coverage must be JSON");

    for key in [
        "config",
        "configWarnings",
        "trees",
        "dispatchMeaning",
        "unmeasured",
    ] {
        assert!(
            v.get(key).is_some(),
            "coverage must always carry `{key}`: {v}"
        );
    }
    let exts = v["trees"][0]["extensions"].as_array().expect("extensions");
    let row = |ext: &str| {
        exts.iter()
            .find(|e| e["ext"] == ext)
            .unwrap_or_else(|| panic!("no `{ext}` row: {exts:?}"))
    };
    assert_eq!(row("ts")["structural"], 1, "{exts:?}");
    assert_eq!(row("md")["lexicalOnly"], 1, "{exts:?}");
    // jsonc appears too (the config init_config wrote) — lexical-only, and that is fine; what this
    // test pins is that the two planted files landed in DIFFERENT dispatch classes.
    assert_eq!(v["unmeasured"][0]["axis"], "recall", "{v}");
    assert!(
        v["trees"][0]["joinVisibility"].as_str().is_some(),
        "join visibility must be a sentence: {v}"
    );
}

#[test]
fn facts_is_byte_stable_and_carries_the_whole_join_for_multiple_trees() {
    let dir = TempDir::new("zzop-facts-multi");
    let config = manifest_fixture(&dir, None);
    let first = run(&["facts", "--config", config.to_str().unwrap()]);
    assert!(first.status.success(), "stderr: {}", stderr(&first));
    let second = run(&["facts", "--config", config.to_str().unwrap()]);
    assert_eq!(
        stdout(&first),
        stdout(&second),
        "the same input must produce the same bytes"
    );
    let v: serde_json::Value = serde_json::from_str(&stdout(&first)).expect("facts must be JSON");
    assert_eq!(
        v["trees"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["sourceId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["fe", "be"],
        "tree order follows the request, not a re-sort (the crossLayer buckets are accumulated in \
         that same order)"
    );
    // All seven join buckets are materialized — the adequacy substrate. `unprovidedConsumes` carries
    // the fixture's dangling `/api/users` call, with the site a rule program would report.
    for bucket in [
        "edges",
        "unconsumedProvides",
        "unprovidedConsumes",
        "unresolvedConsumes",
        "externalConsumes",
        "ambiguousConsumes",
        "hostRekeyCounts",
    ] {
        assert!(
            v["crossLayer"][bucket].is_array(),
            "crossLayer.{bucket} must be materialized: {v}"
        );
    }
    let dangling = &v["crossLayer"]["unprovidedConsumes"][0];
    assert_eq!(dangling["key"], "GET /api/users", "{v}");
    assert_eq!(dangling["source"], "fe", "{v}");
    assert_eq!(dangling["file"], "src/api.ts", "{v}");
    // The `be` tree extracted nothing joinable — the positive blindness fact that keeps a rule author
    // from reading "no edges" as "nothing is wired".
    assert_eq!(
        v["trees"][1]["coverage"]["joinContributionZero"], true,
        "{v}"
    );
}

#[test]
fn facts_argument_shapes_are_usage_errors_like_every_sibling() {
    // Same shared argv parser as `cross`/`manifest`, only the arity floor differs: the
    // silent-narrowing traps (a path AFTER `--config` would be dropped; a dash-shaped path is never a
    // path) close identically.
    let out = run(&["facts"]);
    assert_eq!(out.status.code(), Some(2), "no source is a usage error");
    assert!(
        stderr(&out).contains("usage: zzop facts"),
        "{}",
        stderr(&out)
    );
    let out = run(&["facts", "--config", "x.jsonc", "./extra"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("no extra paths"), "{}", stderr(&out));
    let out = run(&["facts", "--nope"]);
    assert_eq!(out.status.code(), Some(2), "a flag is never a path");
    // A nonexistent path is a RUNTIME failure (exit 1) — the argument was well-formed.
    let out = run(&["facts", "./definitely-not-here"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("path does not exist"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn graph_emits_a_mermaid_document_not_json_and_is_byte_stable() {
    // The only subcommand whose product is not JSON: its consumer is a RENDERER. End to end through
    // the real binary, over the same two-tree fixture `manifest`/`facts` use, so the dangling
    // `/api/users` call lands in `unprovidedConsumes` and reaches the picture as a node.
    let dir = TempDir::new("zzop-graph");
    let config = manifest_fixture(&dir, None);
    let first = run(&["graph", "--config", config.to_str().unwrap()]);
    assert!(first.status.success(), "stderr: {}", stderr(&first));
    let text = stdout(&first);
    assert!(
        serde_json::from_str::<serde_json::Value>(&text).is_err(),
        "graph emits mermaid text, not JSON: {text}"
    );
    assert!(text.starts_with("flowchart LR\n"), "{text}");
    // Both analyzed trees keep a subgraph, including the one that contributed nothing to the join —
    // an absent subgraph would read as "this tree is fine".
    assert!(text.contains("subgraph s0[\"be\"]"), "{text}");
    assert!(text.contains("subgraph s1[\"fe\"]"), "{text}");
    assert!(text.contains("extracted no joinable io"), "{text}");
    assert!(
        text.contains("unprovided · http GET /api/users"),
        "the dangling call must reach the picture as a labelled node: {text}"
    );
    // The census is always present, capped or not, and always covers all six graph-shaped buckets.
    for bucket in [
        "edges",
        "unconsumedProvides",
        "unprovidedConsumes",
        "unresolvedConsumes",
        "externalConsumes",
        "ambiguousConsumes",
    ] {
        assert!(text.contains(&format!("%%   {bucket}: ")), "{text}");
    }
    // What the format cannot carry is stated, never left to inference.
    assert!(text.contains("NOT rendered by this surface"), "{text}");
    let second = run(&["graph", "--config", config.to_str().unwrap()]);
    assert_eq!(
        text,
        stdout(&second),
        "the same input must produce the same bytes"
    );
}

#[test]
fn graph_truncation_and_scope_are_disclosed_through_the_real_binary() {
    let dir = TempDir::new("zzop-graph-top");
    let config = manifest_fixture(&dir, None);
    // `--top 0` draws nothing: the disclosure is then the ONLY honest content, and it must survive
    // into the picture as a node, not just as a `%%` comment a renderer drops.
    let out = run(&["graph", "--config", config.to_str().unwrap(), "--top", "0"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("%% top: 0 drawn relations per bucket"),
        "{text}"
    );
    assert!(
        text.contains("TRUNCATED") && text.contains(":::note"),
        "a capped graph must carry a VISIBLE truncation node: {text}"
    );
    assert!(
        !text.contains("unprovided · http GET /api/users"),
        "nothing is drawn at --top 0: {text}"
    );
    let out = run(&[
        "graph",
        "--config",
        config.to_str().unwrap(),
        "--scope",
        "nothing-matches-this",
    ]);
    let text = stdout(&out);
    assert!(text.contains("%% scope: nothing-matches-this"), "{text}");
    assert!(
        text.contains("SCOPED to 'nothing-matches-this'"),
        "a filtered graph must say so on the canvas: {text}"
    );
}

#[test]
fn graph_argument_shapes_are_usage_errors_like_every_sibling() {
    // The two optional knobs are lifted out of argv before the SHARED trees parser runs, so `graph`
    // inherits every silent-narrowing guard its siblings have — and a malformed knob is an
    // argument-shape mistake (exit 2), never a silently-ignored option.
    let out = run(&["graph"]);
    assert_eq!(out.status.code(), Some(2), "no source is a usage error");
    assert!(
        stderr(&out).contains("usage: zzop graph"),
        "{}",
        stderr(&out)
    );
    let out = run(&["graph", ".", "--top", "not-a-number"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("--top needs a non-negative integer"),
        "{}",
        stderr(&out)
    );
    let out = run(&["graph", ".", "--scope"]);
    assert_eq!(out.status.code(), Some(2), "a knob without a value");
    assert!(stderr(&out).contains("needs a value"), "{}", stderr(&out));
    let out = run(&["graph", "--config", "x.jsonc", "./extra"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("no extra paths"), "{}", stderr(&out));
    let out = run(&["graph", "--nope"]);
    assert_eq!(out.status.code(), Some(2), "a flag is never a path");
    // A nonexistent path is a RUNTIME failure (exit 1) — the argument was well-formed.
    let out = run(&["graph", "./definitely-not-here"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("path does not exist"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn init_writes_the_same_bytes_the_contract_surface_serves() {
    // Seals the whole point of ⑦'s shape: `init` GENERATES nothing. What lands on disk is byte-identical
    // to what `contract config-template` prints and MCP `resources/read` serves, because all three read
    // one embedded document — so a template edit can never reach two surfaces and miss the third.
    let dir = TempDir::new("zzop-init-write");
    let out = run_in(dir.path(), &["init"]);
    assert!(
        out.status.success(),
        "`zzop init` must exit 0: {}",
        stderr(&out)
    );
    let written = std::fs::read_to_string(dir.path().join("zzop.config.jsonc"))
        .expect("`zzop init` must create the config file");
    let printed = stdout(&run(&["contract", "config-template"]));
    assert_eq!(
        written, printed,
        "the file and the contract resource must be one document"
    );
}

#[test]
fn init_refuses_to_overwrite_without_force_and_obeys_it_with() {
    // The old JS `init`'s safety property, restored with this repo's exit-code contract: clobbering a
    // config someone HAND-WROTE is the one way this subcommand can destroy work, so the refusal is a
    // runtime failure (exit 1 — the arguments were well-formed) and the existing bytes stay untouched.
    let dir = TempDir::new("zzop-init-force");
    dir.write("zzop.config.jsonc", "{ \"roots\": [\"./mine\"] }");
    let out = run_in(dir.path(), &["init"]);
    assert_eq!(out.status.code(), Some(1), "a refusal is a runtime failure");
    assert!(stderr(&out).contains("--force"), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("zzop.config.jsonc")).unwrap(),
        "{ \"roots\": [\"./mine\"] }",
        "the refused run must not have touched the file"
    );
    let out = run_in(dir.path(), &["init", "--force"]);
    assert!(
        out.status.success(),
        "`--force` overwrites: {}",
        stderr(&out)
    );
    assert!(
        std::fs::read_to_string(dir.path().join("zzop.config.jsonc"))
            .unwrap()
            .contains("zzop configuration")
    );
}

#[test]
fn init_rejects_any_argument_that_is_not_force() {
    // `--force` is the only argument, so anything else is an argument-shape mistake (exit 2) rather
    // than a silently ignored option — the same never-swallow rule every sibling subcommand carries.
    // `-h` is deliberately NOT in this list any more: a help request is answered (exit 0, stdout) by
    // the help gate before this branch parses argv — see
    // `every_subcommand_answers_its_own_help_request_on_stdout_exit_zero`.
    for arg in ["--nope", "adapter"] {
        let out = run(&["init", arg]);
        assert_eq!(out.status.code(), Some(2), "`zzop init {arg}` must exit 2");
        assert!(
            stderr(&out).contains("usage: zzop init"),
            "{}",
            stderr(&out)
        );
    }
}

#[test]
fn a_freshly_initialized_repo_analyzes_with_no_config_warnings() {
    // The end-to-end honesty pin: run `init`, then analyze — the starter file must produce ZERO
    // unknown-key warnings from the same run that reads it. A template naming a key the front end does
    // not know would greet its author with a warning about the file zzop itself just wrote.
    let dir = TempDir::new("zzop-init-analyze");
    dir.write("src/app.ts", "export const x = 1;\n");
    assert!(run_in(dir.path(), &["init"]).status.success());
    let out = run_in(dir.path(), &["analyze", "."]);
    assert!(out.status.success(), "analyze after init: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        !text.contains("unknown config key"),
        "the file `zzop init` wrote must not warn on its own first run: {text}"
    );
}

/// D15, at the product boundary: an analysis lane pointed at a tree with no config REFUSES, and does so
/// as a runtime/environment failure (exit 1) rather than a usage error (exit 2) — the argv was fine, the
/// prerequisite was not. Pinned through the real binary because this is the first thing a new user hits.
#[test]
fn an_analysis_lane_without_a_config_refuses_with_exit_one_and_names_the_template() {
    let dir = TempDir::new("zzop-no-config");
    dir.write("src/api.ts", "export const a = 1;\n");

    let out = run(&["analyze", dir.path().to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a missing config is an environment failure (1), not a usage error (2); stderr: {}",
        stderr(&out)
    );
    assert!(
        stdout(&out).is_empty(),
        "a refusal must print nothing on stdout, got: {}",
        stdout(&out)
    );
    let err = stderr(&out);
    assert!(
        err.contains("`config-template` contract document"),
        "the refusal must name the artifact both hosts can serve: {err}"
    );
    // Neither host's spelling may appear: this same sentence is what an MCP client receives.
    assert!(
        !err.contains("zzop init"),
        "CLI-only vocabulary leaked: {err}"
    );
    assert!(
        !err.contains("resources/read"),
        "MCP-only vocabulary leaked: {err}"
    );

    // ...and `init` is the way out: the same tree analyzes once the starter config lands.
    init_config(dir.path());
    let after = run(&["analyze", dir.path().to_str().unwrap()]);
    assert!(
        after.status.success(),
        "the starter config must be enough to analyze, stderr: {}",
        stderr(&after)
    );
}

/// The lanes that must keep working WITHOUT a config, because they are how a user gets one (or asks the
/// binary about itself). Listed mechanically rather than described: a lane added to the required set by
/// accident shows up here as a failure instead of as a support question.
#[test]
fn the_config_making_and_self_describing_lanes_still_run_without_a_config() {
    let dir = TempDir::new("zzop-no-config-exempt");
    for argv in [
        vec!["contract"],
        vec!["contract", "config-template"],
        vec!["explain", "no-explicit-any"],
        vec!["version"],
        vec!["help"],
    ] {
        let out = run_in(dir.path(), &argv);
        assert!(
            out.status.success(),
            "`zzop {}` must not require a config, stderr: {}",
            argv.join(" "),
            stderr(&out)
        );
    }
    // `init` is the same class and is exercised for its write effect rather than just its status.
    init_config(dir.path());
}

/// `zzop file --source-id <id>` — the flag that closes a HOST-PARITY hole, not a convenience.
///
/// `file_summary` has always taken a `source_id`, and the `check_file` MCP tool has always passed the
/// caller's. This binary passed `None` on every path until 2026-07-28, so a question the MCP host could
/// ask had no CLI spelling at all — against this repo's hard constraint that both hosts answer alike.
///
/// The reply is what made it visible rather than merely asymmetric: when several trees declare the same
/// relative path, the answer names one tree and lists the rest in `otherTrees`. That is a POINTER, and
/// until this flag existed the CLI reply pointed somewhere its own caller could not go. The fixture
/// below is that exact shape — `cases/` declares four trees and three of them have an `index.ts`.
#[test]
fn file_source_id_selects_which_tree_answers_when_several_share_a_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = root.join("cases/zzop.config.jsonc");
    let config = config.to_str().expect("fixture path is utf-8");

    // No flag: one tree answers and the others are DISCLOSED. This half must not change — the flag is
    // additive, and a caller who never passes it sees exactly what they saw before.
    let bare = run(&["file", "index.ts", "--config", config]);
    assert_eq!(bare.status.code(), Some(0), "{}", stderr(&bare));
    let v: serde_json::Value = serde_json::from_str(&stdout(&bare)).expect("file reply is JSON");
    let picked = v["sourceId"].as_str().expect("a tree answered").to_string();
    let others: Vec<String> = v["otherTrees"]
        .as_array()
        .expect("otherTrees is present when the path is shared")
        .iter()
        .filter_map(|t| t.as_str().map(str::to_string))
        .collect();
    assert!(
        !others.is_empty(),
        "fixture no longer has a shared relative path, so this test would prove nothing: {v}"
    );

    // The pointer is now followable: ask for one of the trees the reply itself named.
    let target = &others[0];
    let picked_other = run(&[
        "file",
        "index.ts",
        "--source-id",
        target,
        "--config",
        config,
    ]);
    assert_eq!(
        picked_other.status.code(),
        Some(0),
        "{}",
        stderr(&picked_other)
    );
    let v2: serde_json::Value =
        serde_json::from_str(&stdout(&picked_other)).expect("file reply is JSON");
    assert_eq!(v2["sourceId"].as_str(), Some(target.as_str()), "{v2}");
    assert_ne!(
        v2["sourceId"].as_str(),
        Some(picked.as_str()),
        "asking for a different tree must not return the default one: {v2}"
    );
    // Once the tree is pinned there is nothing left to disambiguate. The key is OMITTED rather than
    // emitted empty — measured, not assumed: this assertion was written as  first and the run
    // said otherwise, which is the only reason it now states the real shape.
    assert!(
        v2["otherTrees"].as_array().is_none_or(|a| a.is_empty()),
        "a pinned query has no alternatives to name: {v2}"
    );
}

/// An unknown `--source-id` filters every tree out and lands in `not-found` — the same shape the MCP
/// tool produces for the same input, because both hosts hand the value to one shared function. It is a
/// VERDICT, not a usage error: the arguments were well-formed, the tree just is not there.
#[test]
fn file_source_id_that_names_no_tree_is_a_not_found_verdict_not_an_argument_error() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = root.join("cases/zzop.config.jsonc");
    let out = run(&[
        "file",
        "index.ts",
        "--source-id",
        "no-such-tree",
        "--config",
        config.to_str().expect("fixture path is utf-8"),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("file reply is JSON");
    assert_eq!(v["verdict"].as_str(), Some("not-found"), "{v}");
}

/// A value-taking flag with no value is an argument-shape error (exit 2), never a silently-ignored
/// option — the same contract `graph`'s knobs follow.
#[test]
fn file_source_id_without_a_value_is_a_usage_error() {
    let out = run(&["file", "index.ts", "--source-id"]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    assert!(stderr(&out).contains("--source-id"), "{}", stderr(&out));
}
