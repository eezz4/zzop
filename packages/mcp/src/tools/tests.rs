//! Schema pins (`tools/list`) and `tools/call` dispatch tests — the MCP-surface half of this crate's
//! tool-surface coverage. `zzop-host`'s own `tools/tests.rs` (crates/host/src/tools/tests.rs) pins the
//! shared `analyze`/`cross_repo`/`check_endpoint`/`validate_*` handlers directly (the functions the CLI
//! twin subcommands also call); this file drives the same handlers only through the real MCP `tools/
//! call` dispatch (`super::call`) and the `tools/list` schema (`super::list`), so the wire-shape
//! boundary itself — argument-name mapping, `isError` framing, schema `required`/`oneOf` — gets covered
//! end to end, not just the handler logic underneath it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_packs_loaded_entries(loaded: &serde_json::Value, context: &str) {
    let arr = loaded
        .as_array()
        .unwrap_or_else(|| panic!("{context}: packsLoaded must be an array, got: {loaded}"));
    assert!(
        !arr.is_empty(),
        "{context}: zero-config injects the bundled packs, so packsLoaded must be non-empty"
    );
    for p in arr {
        assert!(p["id"].is_string(), "{context}: entry missing id: {p}");
        assert!(p["rules"].is_u64(), "{context}: entry missing rules: {p}");
        assert_eq!(
            p["source"], "inline",
            "{context}: zero-config bundled packs arrive as inline packDefs"
        );
    }
    // Deterministic order: sorted by id.
    let ids: Vec<&str> = arr.iter().filter_map(|p| p["id"].as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "{context}: packsLoaded must be id-sorted");
}

/// Pins the `tools/list` surface: tool names, each schema's `required` array, and the
/// source-exclusivity `oneOf` constraints — so the schema surface cannot drift silently (it had
/// zero test coverage before this pin). Values, not just presence: a renamed tool, a dropped
/// `required` entry, or a loosened `oneOf` branch all fail here by name.
#[test]
fn tools_list_pins_names_required_arrays_and_source_exclusivity() {
    let list = super::list();
    let tools = list["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(
        names,
        [
            "analyze_repo",
            "cross_repo",
            "check_endpoint",
            "analyze_envelope",
            "validate_envelope",
            "validate_rule_pack"
        ]
    );
    let schema = |name: &str| -> &serde_json::Value {
        &tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("tool {name} listed"))["inputSchema"]
    };

    assert_eq!(
        schema("analyze_repo")["required"],
        serde_json::json!(["path"])
    );
    assert_eq!(
        schema("analyze_envelope")["required"],
        serde_json::json!(["envelopeJson"])
    );
    assert_eq!(
        schema("validate_envelope")["required"],
        serde_json::json!(["envelopeJson"])
    );
    assert_eq!(
        schema("validate_rule_pack")["required"],
        serde_json::json!(["packJson"])
    );

    // cross_repo: paths XOR configPath, expressed as `oneOf` (no top-level `required` — neither
    // source is individually required).
    let cross = schema("cross_repo");
    assert!(cross.get("required").is_none());
    assert_eq!(
        cross["oneOf"],
        serde_json::json!([{ "required": ["paths"] }, { "required": ["configPath"] }])
    );

    // check_endpoint: `pattern` always, plus exactly ONE of path/paths/configPath.
    let endpoint = schema("check_endpoint");
    assert_eq!(endpoint["required"], serde_json::json!(["pattern"]));
    assert_eq!(
        endpoint["oneOf"],
        serde_json::json!([
            { "required": ["pattern", "path"] },
            { "required": ["pattern", "paths"] },
            { "required": ["pattern", "configPath"] }
        ])
    );
    // The schema under-declared `pattern`'s non-emptiness — behavior already enforces it
    // (`zzop-facade`'s queryIo() rejects an empty pattern), the schema just never said so.
    assert_eq!(endpoint["properties"]["pattern"]["minLength"], 1);

    // `limit`'s schema minimum is 0 (not 1): `limit: 0` is a legal "counts only" query.
    assert_eq!(schema("analyze_repo")["properties"]["limit"]["minimum"], 0);
    assert_eq!(
        schema("analyze_repo")["properties"]["limit"]["maximum"],
        1000
    );
}

/// README-vs-tools-list drift pin: the tools table in `crates/host/README.md` (the shared reference
/// doc every host's tool surface is documented against) went stale once (`analyze_envelope` shipped
/// without a row) with nothing to catch it — closes the same drift class the surface-parity registry
/// closes for output fields. Kept a simple name-presence substring check (like the surface-parity JS
/// test does for field names), not a full table-shape parser: the goal is "a new tool that isn't in
/// the README fails the build," not byte-parity with the markdown table.
#[test]
fn every_tool_name_from_tools_list_appears_in_the_readme() {
    const README: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/host/README.md"
    ));
    let list = super::list();
    let tools = list["tools"].as_array().expect("tools array");
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name is a string");
        assert!(
            README.contains(name),
            "tool `{name}` from tools/list is missing from crates/host/README.md's tools table — \
             add a row (or the README will silently drift stale again)"
        );
    }
}

#[test]
fn validate_rule_pack_tool_reports_shape_verdicts_and_never_is_error_on_bad_input() {
    // A structurally valid pack (a real bundled one) -> {valid: true, issues: []}.
    let bundled = zzop_config::BUNDLED_PACK_SOURCES[0].1;
    let params = serde_json::json!({
        "name": "validate_rule_pack",
        "arguments": { "packJson": bundled }
    });
    let reply = super::call(Some(&params));
    assert!(reply.get("isError").is_none(), "got: {reply}");
    let report: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(report["valid"], true, "got: {report}");

    // A broken pack (missing `rules`) -> a NORMAL reply carrying {valid: false, issues: [named]},
    // not an isError — invalid input is the tool's answer, not its failure.
    let params = serde_json::json!({
        "name": "validate_rule_pack",
        "arguments": { "packJson": "{\"id\": \"p\"}" }
    });
    let reply = super::call(Some(&params));
    assert!(reply.get("isError").is_none(), "got: {reply}");
    let report: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(report["valid"], false, "got: {report}");
    assert!(
        report["issues"][0]
            .as_str()
            .unwrap()
            .contains("missing field `rules`"),
        "got: {report}"
    );

    // A missing `packJson` argument IS a tool-level error (the caller's call shape is wrong).
    let params = serde_json::json!({ "name": "validate_rule_pack", "arguments": {} });
    let reply = super::call(Some(&params));
    assert_eq!(reply["isError"], true, "got: {reply}");
}

// --- Boundary-value torture round: wrong-JSON-type arguments must be a named error, never a
// --- silent fallback (see `crate::args`'s module doc). Every case below goes through the REAL
// --- `tools/call` dispatch (`super::call`), not a lower-level unit, so the sweep proves the wiring
// --- end to end.

fn call_tool(name: &str, arguments: serde_json::Value) -> serde_json::Value {
    let params = serde_json::json!({ "name": name, "arguments": arguments });
    super::call(Some(&params))
}

fn error_text(reply: &serde_json::Value) -> String {
    assert_eq!(reply["isError"], true, "expected isError, got: {reply}");
    reply["content"][0]["text"].as_str().unwrap().to_string()
}

#[test]
fn analyze_repo_rejects_a_non_string_path() {
    let reply = call_tool("analyze_repo", serde_json::json!({ "path": 5 }));
    let err = error_text(&reply);
    assert!(
        err.contains("`path` must be a string (got 5)"),
        "got: {err}"
    );
}

#[test]
fn cross_repo_rejects_a_non_array_paths_and_a_non_string_element() {
    let reply = call_tool("cross_repo", serde_json::json!({ "paths": "not-an-array" }));
    let err = error_text(&reply);
    assert!(
        err.contains("`paths` must be an array of strings"),
        "got: {err}"
    );

    let reply = call_tool("cross_repo", serde_json::json!({ "paths": ["ok", 7] }));
    let err = error_text(&reply);
    assert!(
        err.contains("`paths` entries must be strings (got 7)"),
        "got: {err}"
    );
}

#[test]
fn cross_repo_rejects_a_non_string_config_path() {
    let reply = call_tool("cross_repo", serde_json::json!({ "configPath": true }));
    let err = error_text(&reply);
    assert!(
        err.contains("`configPath` must be a string (got true)"),
        "got: {err}"
    );
}

#[test]
fn check_endpoint_rejects_non_string_pattern_path_and_config_path() {
    let reply = call_tool("check_endpoint", serde_json::json!({ "pattern": 1 }));
    assert!(
        error_text(&reply).contains("`pattern` must be a string (got 1)"),
        "got: {reply}"
    );

    let reply = call_tool(
        "check_endpoint",
        serde_json::json!({ "pattern": "x", "path": null, "configPath": 3 }),
    );
    assert!(
        error_text(&reply).contains("`configPath` must be a string (got 3)"),
        "got: {reply}"
    );
}

/// `docs/NORMALIZED_AST.md`'s worked example (also served as the `example-envelope` MCP contract
/// resource, `zzop_host::embedded`) — a minimal, valid, one-file v1 envelope.
const EXAMPLE_ENVELOPE: &str = include_str!("../../../../examples/jsp-envelope.example.json");

#[test]
fn analyze_envelope_tool_runs_mode_a_end_to_end_through_the_real_tool_call() {
    let reply = call_tool(
        "analyze_envelope",
        serde_json::json!({ "envelopeJson": EXAMPLE_ENVELOPE }),
    );
    assert!(reply.get("isError").is_none(), "got: {reply}");
    let v: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(v.get("findings").is_some(), "got: {v}");
    assert!(v.get("coverage").is_some(), "got: {v}");
    assert_packs_loaded_entries(&v["packsLoaded"], "analyze_envelope");
    // Envelope mode has no filesystem root/config file — unlike analyze_repo, neither key rides.
    assert!(v.get("path").is_none(), "got: {v}");
    assert!(v.get("config").is_none(), "got: {v}");
}

#[test]
fn analyze_envelope_tool_requires_the_envelope_json_argument() {
    let reply = call_tool("analyze_envelope", serde_json::json!({}));
    let err = error_text(&reply);
    assert!(err.contains("`envelopeJson`"), "got: {err}");
}

#[test]
fn validate_envelope_and_validate_rule_pack_reject_non_string_json_arguments() {
    let reply = call_tool(
        "validate_envelope",
        serde_json::json!({ "envelopeJson": 1 }),
    );
    assert!(
        error_text(&reply).contains("`envelopeJson` must be a string (got 1)"),
        "got: {reply}"
    );
    let reply = call_tool(
        "validate_rule_pack",
        serde_json::json!({ "packJson": false }),
    );
    assert!(
        error_text(&reply).contains("`packJson` must be a string (got false)"),
        "got: {reply}"
    );
}

#[test]
fn analyze_repo_rejects_an_out_of_range_or_wrong_type_limit_and_a_non_string_severity() {
    let dir = TempDir::new("zzop-mcp-arg-sweep-limit");
    dir.write("a.ts", "export const a = 1;\n");
    let path = dir.path().display().to_string();

    for bad_limit in [
        serde_json::json!(-1),
        serde_json::json!(1001),
        serde_json::json!(999_999),
        serde_json::json!("50"),
        serde_json::json!(3.7),
    ] {
        let reply = call_tool(
            "analyze_repo",
            serde_json::json!({ "path": path, "limit": bad_limit }),
        );
        let err = error_text(&reply);
        assert!(
            err.contains("zzop error: limit must be an integer between 0 and 1000"),
            "limit {bad_limit}: got {err}"
        );
    }

    // limit: 0 must be ACCEPTED (a legal "counts only" query), never rejected.
    let reply = call_tool(
        "analyze_repo",
        serde_json::json!({ "path": path, "limit": 0 }),
    );
    assert!(reply.get("isError").is_none(), "got: {reply}");
    let v: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(v["findings"]["shown"], serde_json::json!([]));

    // A NUMBER severity must hit the same rejection as an unknown STRING severity, not silently
    // drop the filter.
    let reply = call_tool(
        "analyze_repo",
        serde_json::json!({ "path": path, "severity": 5 }),
    );
    let err = error_text(&reply);
    assert!(err.contains("zzop error: unknown severity 5"), "got: {err}");
}

#[test]
fn analyze_repo_rule_filter_zero_match_note_fires_end_to_end_through_the_real_tool_call() {
    let dir = TempDir::new("zzop-mcp-rule-note-e2e");
    dir.write("a.ts", "export const a = 1;\n");
    let reply = call_tool(
        "analyze_repo",
        serde_json::json!({ "path": dir.path().display().to_string(), "rule": "nonexistent-xyz" }),
    );
    assert!(reply.get("isError").is_none(), "got: {reply}");
    let v: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
    let note = v["findings"]["note"]
        .as_str()
        .unwrap_or_else(|| panic!("note must be present end-to-end through tools/call, got: {v}"));
    assert!(note.contains("nonexistent-xyz"), "got: {note}");
}
