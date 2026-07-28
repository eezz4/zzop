//! Schema pins (`tools/list`) and `tools/call` dispatch tests — the MCP-surface half of this crate's
//! tool-surface coverage. `zzop-summary`'s own `tests/host_dispatch.rs` pins the
//! shared `analyze_summary`/`cross_summary`/`endpoint_summary`/validator entry points directly (the
//! functions the CLI twin subcommands also call); this file drives the same handlers only through the
//! real MCP `tools/call` dispatch (`super::call`) and the `tools/list` schema (`super::list`), so the
//! wire-shape boundary itself — argument-name mapping, `isError` framing, schema `required`/`oneOf` —
//! gets covered end to end, not just the handler logic underneath it.

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

    /// Drops the starter config into this fixture — the same bytes `zzop init` writes and the
    /// `config-template` resource serves. Every analysis lane requires a config as of 2026-07-27, on
    /// BOTH hosts (that identical entry behaviour is the point), so an analyzable fixture needs one.
    fn write_starter_config(&self) {
        self.write(
            zzop_config::DEFAULT_CONFIG_FILENAME,
            zzop_config::template::CONFIG_TEMPLATE_JSONC,
        );
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
            "check_file",
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

    // analyze_repo: `path` XOR `configPath` since 2026-07-27 (the CLI twin's `zzop analyze --config`
    // arrived in the same change), so like cross_repo it has NO top-level `required` — neither source
    // is individually required — and the exclusivity rides `oneOf` instead.
    let analyze = schema("analyze_repo");
    assert!(analyze.get("required").is_none());
    assert_eq!(
        analyze["oneOf"],
        serde_json::json!([{ "required": ["path"] }, { "required": ["configPath"] }])
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

    // check_file: `target` always, plus exactly ONE of path/paths/configPath — the same shape
    // check_endpoint uses, because it is the same kind of question with a different target axis.
    let file = schema("check_file");
    assert_eq!(file["required"], serde_json::json!(["target"]));
    assert_eq!(
        file["oneOf"],
        serde_json::json!([
            { "required": ["target", "path"] },
            { "required": ["target", "paths"] },
            { "required": ["target", "configPath"] }
        ])
    );
    assert_eq!(file["properties"]["target"]["minLength"], 1);

    // `limit`'s schema minimum is 0 (not 1): `limit: 0` is a legal "counts only" query.
    assert_eq!(schema("analyze_repo")["properties"]["limit"]["minimum"], 0);
    assert_eq!(
        schema("analyze_repo")["properties"]["limit"]["maximum"],
        1000
    );
}

/// README-vs-tools-list drift pin: the tools table in `packages/README.md` (the shared reference
/// doc every host's tool surface is documented against) went stale once (`analyze_envelope` shipped
/// without a row) with nothing to catch it — closes the same drift class the surface-parity registry
/// closes for output fields. Kept a simple name-presence substring check (like the surface-parity JS
/// test does for field names), not a full table-shape parser: the goal is "a new tool that isn't in
/// the README fails the build," not byte-parity with the markdown table.
#[test]
fn every_tool_name_from_tools_list_appears_in_the_readme() {
    const README: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../README.md"));
    let list = super::list();
    let tools = list["tools"].as_array().expect("tools array");
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name is a string");
        assert!(
            README.contains(name),
            "tool `{name}` from tools/list is missing from packages/README.md's tools table — \
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
/// resource, `zzop_summary::contracts`) — a minimal, valid, one-file v1 envelope.
const EXAMPLE_ENVELOPE: &str = include_str!("../../../../docs/contracts/example-envelope.json");

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
    dir.write_starter_config();
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
    dir.write_starter_config();
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

/// Every tool `tools/list` advertises must have a `call()` dispatch arm.
///
/// # Why this is its own test, and why nothing caught the gap
/// `list()` and `call()` are two hand-maintained enumerations of the same set — a schema table in
/// `definitions.rs` and a `match name` in `tools.rs`. Nothing tied them together. The D22 sweep
/// (2026-07-28) planted a tool into `list()` alone and ran the whole crate's suite: the README-parity
/// test and one hand-list schema pin failed for their own unrelated reasons, and **the missing
/// dispatch itself went unreported by every test in the repo**. An agent reading `tools/list` would
/// have called a tool the server advertises and been told it does not exist.
///
/// # The discriminator
/// `call()`'s fallthrough arm is the only place that produces `unknown tool: <name>`; every real arm
/// fails, if at all, on its own argument validation. So calling each advertised tool with NO arguments
/// and asserting the reply is not that one string separates "no dispatch arm" from "arm exists and
/// rejected my empty arguments" — without needing valid arguments for seven different tools.
///
/// The subject set is `list()` itself, never a list spelled here: a tool absent from a hand-typed
/// table is a tool nobody checks, which is the whole defect class this test was written during.
#[test]
fn every_advertised_tool_has_a_call_dispatch_arm() {
    let listed = super::list();
    let names: Vec<&str> = listed["tools"]
        .as_array()
        .expect("tools/list must return a tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("every tool has a name"))
        .collect();
    assert!(
        !names.is_empty(),
        "tools/list advertised nothing — this test would then vouch for nothing"
    );

    let mut undispatched = Vec::new();
    for name in &names {
        let reply = super::call(Some(&serde_json::json!({ "name": name, "arguments": {} })));
        let text = reply["content"][0]["text"].as_str().unwrap_or_default();
        if text.contains(&format!("unknown tool: {name}")) {
            undispatched.push(*name);
        }
    }
    assert!(
        undispatched.is_empty(),
        "these tools are advertised by tools/list but have no arm in `call()`'s match, so calling one \
         answers `unknown tool`: {undispatched:?}. A tool the server offers and then denies is worse \
         than an absent tool — the client has no way to tell the refusal from a bug in its own request."
    );
}
