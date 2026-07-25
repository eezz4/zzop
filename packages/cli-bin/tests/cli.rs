//! Binary-level tests for the `zzop` CLI binary's argument dispatch (`src/main.rs`, package
//! `zzop-cli-bin`) — the thin layer the shared `zzop-host` crate's own unit tests (`tools/tests.rs`,
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
            format!("zzop {}", zzop_host::server::version()),
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
fn analyze_help_flag_is_a_usage_error_never_a_path() {
    // The blind-test failure this pins: `analyze --help` used to be swallowed as a path and die
    // with "path does not exist: --help" (exit 1). A dash-shaped argument in a path position is a
    // usage error, exit 2.
    let out = run(&["analyze", "--help"]);
    assert_eq!(out.status.code(), Some(2), "usage errors exit 2");
    let err = stderr(&out);
    assert!(err.contains("usage: zzop analyze"), "got: {err}");
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
    // The terminal lane to the embedded authoring contracts: `contract` with no name lists all ten
    // (name + description + mime, human-readable lines) — a terminal user must never have to
    // reverse-engineer the config surface from error messages while the docs sit inside the binary.
    let out = run(&["contract"]);
    assert!(out.status.success(), "`zzop contract` must exit 0");
    let text = stdout(&out);
    for doc in zzop_host::embedded::CONTRACT_DOCS {
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
    for doc in zzop_host::embedded::CONTRACT_DOCS {
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
// (`zzop_host::explain`). The ambiguous-bare-id lane (a bare id shared by two packs) is NOT pinned
// here: `derived_suppress_markers_are_globally_unique`
// (`crates/engine/tests/rule_contracts/markers.rs`) machine-enforces that every shipped rule id is
// globally unique today, so real bundled data has no bare-id collision to spawn the binary against —
// that lane is instead pinned against a fabricated pack pair in `crates/host/src/explain/tests.rs`
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
    assert!(text.contains("suppress marker: nplus1-ok"), "got: {text}");
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

/// The io-scan lane of the same contract, and the reason it was worth fixing: `http/route-exposure`
/// carries BOTH an `anchor_exclude_pattern` and a `file_exclude_pattern`, and the pre-fix render reported
/// `exclude_pattern: no` for it — the tool lying about its own vetoes, which sends the reader off to file
/// a false-negative report or to build a carve-out that already exists.
#[test]
fn explain_reports_an_io_scan_rules_real_exclusion_fields() {
    let out = run(&["explain", "http/route-exposure"]);
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
        text.contains("suppress marker: route-exposure-ok (in a `//` or `#` line comment"),
        "got: {text}"
    );
}

/// The ATTRIBUTE-gate lane of the same contract. It matters more than the regex lanes: with
/// `require_attr_declared` set and nothing declaring that key, the rule does not run AT ALL, so a reader
/// who cannot see the field reads the resulting silence as a false negative — the exact misreading this
/// section exists to prevent. `reliability/env-outside-config` is the shipped rule that lives on all
/// three fields; it went silent-by-default in v0.24.0, which is when a reader most needs to be told why.
#[test]
fn explain_reports_a_line_scan_rules_attribute_gates() {
    let out = run(&["explain", "reliability/env-outside-config"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("matcher: line-scan"), "got: {text}");
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

/// The G14 class, one namespace over from the `schema/<label>` lane: strings zzop prints under a field
/// literally named `id`/`group` in EVERY analyze reply — a coverage-disclosure class and its taxonomy
/// group (`disclosure[]`) and a recommendation id (`architecture.topRecommendation.id`) — used to come
/// back as "unknown rule id", the tool denying its own output. They are still lookup FAILURES (there is
/// no DSL rule to render), but guided ones that name what the id actually is. The exhaustive sweep over
/// the live registry lives in `crates/host/src/explain/output_ids.rs`; this pins the wire behavior.
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

    let flag_like = run(&["explain", "--help"]);
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
/// be un-explainable. Loads the packs the exact same way `zzop_host::explain` does
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
    let out = run(&["manifest", "--help"]);
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

#[test]
fn help_and_usage_both_name_the_two_new_subcommands() {
    // The usage line and the help elaboration are one const + one function (cli.rs), so they cannot
    // drift — this pins that a new subcommand actually reached BOTH surfaces.
    let help = stdout(&run(&["help"]));
    let usage = stderr(&run(&[]));
    for sub in ["manifest", "diff"] {
        assert!(help.contains(sub), "help must name {sub}: {help}");
        assert!(usage.contains(sub), "usage must name {sub}: {usage}");
    }
}
