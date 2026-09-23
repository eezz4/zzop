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

/// The session FIXED COST — what an agent pays before it can ask anything.
///
/// # This freezes a number, it does not bless one
///
/// Review ledger V124 measured the bet this server makes: an agent receives the whole `tools/list`
/// reply before its first call, and nothing capped it. Whether that size is what this product wants
/// to spend is an open USER decision and this test does not answer it — it answers the other half,
/// which is that the number must not grow while nobody is looking.
///
/// So the ceiling sits deliberately just above today's measurement. A change that pushes past it is
/// not forbidden; it is required to come back here, read this paragraph, and move the number on
/// purpose. That is the same ratchet shape `scripts/check-max-file-lines.sh` uses, and for the same
/// reason: an unbounded surface grows by accident, one reasonable sentence at a time.
///
/// Counted as WIRE bytes (`serde_json::to_string`), because that is what the transport carries and
/// what the agent's context pays for. Measuring the decoded strings undercounts by the JSON escaping.
/// # Moved once, on purpose (2026-09-15, review ledger V226)
///
/// 34,000 -> 36,500, and what the 2,000-odd extra bytes buy is the `module_map` tool: the answer to
/// "what IS this codebase", which until this commit existed only behind `zzop graph --domain dep
/// --fold N` and so was reachable only from a terminal, while this product's named audience is an
/// agent. Measured over the wire the same day, its reply at `fold` 1 on this engine's own tree is
/// 3,032 bytes where `analyze_repo` on the same tree is 30,587 and carries no module map at all —
/// so the fixed cost bought here is recovered by the first orientation question an agent would
/// otherwise answer with the larger reply.
///
/// The description was written at 3,622 bytes and cut to 2,425 BEFORE this number moved, by deleting
/// the per-key definitions the reply's own folded `meaning` already owns. That order is the rule, not
/// an anecdote: the ceiling is raised by what remains after the sentence has been made to earn its
/// place, never to accommodate the first draft.
const SESSION_FIXED_COST_CEILING: usize = 36_500;

#[test]
fn the_session_fixed_cost_has_a_ceiling_and_it_is_not_an_endorsement() {
    let listed = super::list();
    let wire = serde_json::to_string(&listed).expect("tools/list serializes");
    let total = wire.len();

    assert!(
        total <= SESSION_FIXED_COST_CEILING,
        "tools/list is {total} wire bytes, past the {SESSION_FIXED_COST_CEILING}-byte ceiling. \
         Every agent pays this before its first question. If the growth is deliberate, raise the \
         constant in the same commit and say in the message what the extra bytes buy; if it is not, \
         this is the accident the ceiling exists to catch."
    );

    // A ceiling with nothing under it would pass on an empty reply, which is the shape this repo
    // calls a measurement failure rather than a clean result.
    assert!(
        total > SESSION_FIXED_COST_CEILING / 2,
        "tools/list is only {total} wire bytes — far under the ceiling. That is not 'small', it is \
         evidence the surface collapsed; check that every tool is still listed."
    );
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
            "check_coverage",
            "module_map",
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

/// Pins the annotation table's HONESTY, not just its presence. Annotations are advisory UI hints a
/// host may use to skip a confirmation prompt, so a false one quietly bypasses user consent — the
/// dangerous direction here is a tree-analyzing tool claiming `readOnlyHint: true` while the
/// analysis persists its `.zzop/` cache to disk (the front end injects the default beside the
/// honored config — `crates/config/src/mapper/options.rs`; the behavioral half of this claim is
/// asserted by `analyze_repo_actually_writes_the_cache_the_read_only_hint_denies` below, not just
/// spelled). The three text-in/judgment-out tools are the only ones allowed to claim read-only:
/// their lane carries no `cacheDir` in its request (`crates/facade/src/request.rs`) and the MCP arm
/// passes no path to discover a config from. `openWorldHint: false` is pinned for every tool
/// because the workspace has no HTTP client crate at all; if a network call ever arrives, this pin
/// forces the annotation (and the no-network claim it encodes) to be re-judged in the same change.
/// The top-level `title` (2025-06-18 BaseMetadata) and `annotations.title` (older clients'
/// fallback) are pinned equal so the two spellings cannot drift.
#[test]
fn tool_annotations_never_claim_read_only_for_the_cache_writing_tools() {
    let list = super::list();
    let tools = list["tools"].as_array().expect("tools array");
    const CACHE_WRITERS: [&str; 6] = [
        "analyze_repo",
        "cross_repo",
        "check_file",
        "check_endpoint",
        // Runs the same `analyzeTrees` path before post-processing, so the injected `.zzop/cache`
        // default applies to it exactly as to the four above. A visibility REPORT reads read-only,
        // which is precisely why the honesty rule above is a list and not an inference from the name.
        "check_coverage",
        // Same reason one step further: a MAP reads read-only to a caller, and it gets there by
        // running the full analysis first.
        "module_map",
    ];
    const PURE_JUDGES: [&str; 3] = [
        "analyze_envelope",
        "validate_envelope",
        "validate_rule_pack",
    ];
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name is a string");
        let ann = &tool["annotations"];
        assert!(
            ann.is_object(),
            "tool `{name}` has no annotations object — every tool carries title + honest hints"
        );
        assert!(
            ann["title"].as_str().is_some_and(|t| !t.is_empty()),
            "tool `{name}` annotations lack a non-empty title"
        );
        assert_eq!(
            tool["title"], ann["title"],
            "tool `{name}`'s top-level title (2025-06-18 BaseMetadata) and annotations.title \
             (older clients' fallback) must be the same string"
        );
        assert_eq!(
            ann["openWorldHint"],
            serde_json::json!(false),
            "tool `{name}` must pin openWorldHint: false — zzop makes zero network calls"
        );
        assert_eq!(
            ann["idempotentHint"],
            serde_json::json!(true),
            "tool `{name}` is deterministic; a repeat call with the same args adds nothing"
        );
        if CACHE_WRITERS.contains(&name) {
            assert_eq!(
                ann["readOnlyHint"],
                serde_json::json!(false),
                "tool `{name}` writes the `.zzop/` cache — claiming read-only would let a host \
                 skip a confirmation the user should have seen"
            );
            assert_eq!(
                ann["destructiveHint"],
                serde_json::json!(false),
                "tool `{name}`'s writes (and self-evictions) stay inside zzop's own \
                 `.zzop/cache/`, never touching a file a user authored"
            );
        } else {
            assert!(
                PURE_JUDGES.contains(&name),
                "tool `{name}` is in neither annotation class — classify it here explicitly \
                 (measure whether its lane touches disk) before shipping it"
            );
            assert_eq!(
                ann["readOnlyHint"],
                serde_json::json!(true),
                "tool `{name}` is text-in/judgment-out and must claim read-only"
            );
        }
    }
}

/// The behavioral half of the annotation pin above: `readOnlyHint: false` on the tree tools is a
/// claim about disk writes, and this test PERFORMS the write instead of trusting the table — a real
/// `analyze_repo` call over a scratch tree must leave zzop's cache directory behind. If this fails
/// because the front end stopped injecting a cache default (or the starter config stopped naming
/// one), the annotation classification is what must be re-judged in the same change — a green table
/// pin over a lane that no longer writes would be the exact spelling-without-behavior drift the pin
/// alone cannot see.
#[test]
fn analyze_repo_actually_writes_the_cache_the_read_only_hint_denies() {
    let dir = TempDir::new("zzop-mcp-cache-write");
    dir.write("src/a.ts", "export const a = 1;\n");
    dir.write_starter_config();
    let reply = call_tool(
        "analyze_repo",
        serde_json::json!({"path": dir.path().to_string_lossy()}),
    );
    assert_ne!(
        reply["isError"],
        serde_json::json!(true),
        "analyze failed: {reply}"
    );
    assert!(
        dir.path().join(".zzop").is_dir(),
        "analyze_repo left no .zzop/ cache in the analyzed tree — either the cache default moved \
         (re-judge readOnlyHint in the same change) or this fixture no longer triggers an analysis"
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

/// `cross_repo`'s SOURCE-MODE errors ("both" / "neither") must be the shared handler's own words,
/// byte for byte — this arm owns no exclusivity rule of its own.
///
/// It used to. `tools.rs` re-implemented "not both" / "pass one" locally while its three sibling arms
/// passed both optional sources straight through, and the file's own `analyze_repo` comment already
/// stated the principle the outlier broke: *"the shared handler decides: it owns 'exactly one source',
/// so the two hosts cannot drift on which combinations are legal"*. `zzop_config::trees` rejects the
/// same two inputs (pinned from the summary side by `crates/summary/src/cross_test.rs`), so the local
/// copy bought nothing and could only drift — a second sentence for the same judgment, reachable only
/// through MCP, while the CLI twin got the first.
///
/// Comparing against a live `cross_summary` call rather than against literal strings is the point: a
/// literal would be a THIRD copy of the message, and this test would then pin the drift instead of
/// catching it.
#[test]
fn cross_repo_source_mode_errors_are_the_shared_handlers_verbatim() {
    let filters = zzop_summary::FindingFilters::from_args(None).expect("no-args filters");
    let cases: [(serde_json::Value, Vec<String>, Option<&str>); 2] = [
        (
            serde_json::json!({ "paths": ["a", "b"], "configPath": "zzop.config.jsonc" }),
            vec!["a".to_string(), "b".to_string()],
            Some("zzop.config.jsonc"),
        ),
        (serde_json::json!({}), Vec::new(), None),
    ];
    for (arguments, paths, config_path) in cases {
        let shared = zzop_summary::cross_summary(&paths, config_path, &filters)
            .expect_err("both sources, and neither source, are each an error");
        let reply = call_tool("cross_repo", arguments.clone());
        assert_eq!(
            error_text(&reply),
            format!("zzop error: {shared}"),
            "cross_repo({arguments}) answered with words that are not the shared handler's. \
             A source-mode rule spelled a second time in this file is drift surface, not a guard."
        );
    }
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
/// resource, `zzop_summary::contracts`) — a minimal, valid, one-file envelope.
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

/// The subject is `findings.note` reaching a caller through the REAL dispatch, and it is unchanged.
///
/// One incidental line moved on 2026-09-01. This test used to assert `isError` was ABSENT — scaffolding
/// for reading `content[0]` — and that line had quietly become a pin on the exact asymmetry
/// `an_unmatchable_rule_filter_is_an_error_on_this_host_as_it_is_on_the_cli` repairs: `nonexistent-xyz`
/// is an id this run can PROVE could never match, and the `zzop analyze` twin exits 2 on it. What this
/// test actually needs is that the document is still `content[0]` and still parses, which is asserted
/// below and is now the stronger statement of the two.
#[test]
fn analyze_repo_rule_filter_zero_match_note_fires_end_to_end_through_the_real_tool_call() {
    let dir = TempDir::new("zzop-mcp-rule-note-e2e");
    dir.write_starter_config();
    dir.write("a.ts", "export const a = 1;\n");
    let reply = call_tool(
        "analyze_repo",
        serde_json::json!({ "path": dir.path().display().to_string(), "rule": "nonexistent-xyz" }),
    );
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

#[test]
fn a_bare_native_tail_in_the_rule_filter_is_resolved_not_denied() {
    // `god-model` IS the bare form of `schema/god-model`, so the shared warning saying "which is not
    // a native analysis id" was simply false — on this host only, because the CLI resolves the tail in
    // its own pre-check and exits 2 before the shared sentence is ever built. One binary, two verdicts
    // on whether the thing the caller typed exists, and the half that reached an MCP client was wrong.
    // Pins the truthful half; the CLI's own half is pinned in packages/cli-bin/tests/cli.rs.
    let params = serde_json::json!({
        "name": "analyze_repo",
        "arguments": {
            "configPath": "../../cases/trees/api-be/zzop.config.jsonc",
            "rule": "god-model",
            "limit": 0
        }
    });
    let reply = super::call(Some(&params));
    let text = reply["content"][0]["text"].as_str().unwrap_or_default();
    let body: serde_json::Value = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
    let warnings = body["warnings"].as_array().cloned().unwrap_or_default();
    let hit = warnings
        .iter()
        .filter_map(|w| w.as_str())
        .find(|w| w.contains("`rule` filter"))
        .unwrap_or_else(|| panic!("no rule-filter warning in the reply: {text:.400}"));

    assert!(
        hit.contains("schema/god-model"),
        "the warning must name the FULL id the bare form resolves to: {hit}"
    );
    // The falsehood this test exists to keep out.
    assert!(
        !hit.contains("is not a native analysis id"),
        "the reply denies an id that exists: {hit}"
    );
    // Host-neutral: the shared sentence names the id, never how to pass it.
    assert!(
        !hit.contains("--rule"),
        "CLI-only spelling leaked into a shared warning: {hit}"
    );
}

#[test]
fn module_map_refusals_name_the_module_map_and_no_cli_only_lane() {
    // `module_map` and the CLI `map` lane share one helper in `zzop_summary::graph`, and that helper
    // hardcoded the operation name `"graph"` -- a lane `surface-parity.json` declares CLI-only and
    // this binary does not carry. So a client was told to "run graph", which it cannot. Third round of
    // this class here (`zzop pack validate`, then `zzop contract envelope-guide`), and TWO guards are
    // structurally blind to it: the shared-crate vocabulary contract subtracts every file under a
    // CLI-only lane, and its space-free-literal exemption would have skipped the bare word anyway --
    // the same exemption that let `"cross_repo"` leak the other direction and needed a human to catch.
    // A test is therefore the only gate this can have.
    let dir = std::env::temp_dir().join(format!("zzop-mcp-mapop-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("A/web")).unwrap();
    std::fs::create_dir_all(dir.join("A/api")).unwrap();
    std::fs::create_dir_all(dir.join("B")).unwrap();
    std::fs::write(dir.join("A/web/a.ts"), "export const a = 1;\n").unwrap();
    std::fs::write(dir.join("A/api/b.ts"), "export const b = 1;\n").unwrap();
    std::fs::write(dir.join("B/c.ts"), "export const c = 1;\n").unwrap();
    std::fs::write(
        dir.join("A/zzop.config.jsonc"),
        r#"{ "trees": [{ "root": "./web" }, { "root": "./api" }] }"#,
    )
    .unwrap();
    std::fs::write(dir.join("B/zzop.config.jsonc"), r#"{ "roots": ["."] }"#).unwrap();

    let params = serde_json::json!({
        "name": "module_map",
        "arguments": {
            "paths": [dir.join("A").to_string_lossy(), dir.join("B").to_string_lossy()],
            "fold": 2
        }
    });
    let reply = super::call(Some(&params));
    std::fs::remove_dir_all(&dir).ok();

    let text = reply["content"][0]["text"].as_str().unwrap_or_default();
    assert_eq!(
        reply["isError"], true,
        "a path carrying its own tree set must refuse: {reply}"
    );
    assert!(
        text.contains("the module map"),
        "the refusal must name the operation the CALLER asked for: {text}"
    );
    // The load-bearing half. `graph` is CLI-only; naming it here is advice this client cannot take.
    assert!(
        !text.contains("run graph"),
        "a CLI-only lane name reached the MCP wire: {text}"
    );
}

#[test]
fn paths_mode_refusal_reaching_an_mcp_client_names_an_argument_not_a_flag() {
    // Third member of the same class, pinned the same day the gap was found (2026-09-23). "CONFIG
    // MODE" is deliberately not a spelling: on this host the way in is an ARGUMENT name, and a
    // `--config` flag would be advice a client with no shell cannot take.
    let dir = std::env::temp_dir().join(format!("zzop-mcp-paths-mode-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("A/web")).unwrap();
    std::fs::create_dir_all(dir.join("A/api")).unwrap();
    std::fs::create_dir_all(dir.join("B")).unwrap();
    std::fs::write(dir.join("A/web/a.ts"), "export const a = 1;\n").unwrap();
    std::fs::write(dir.join("A/api/b.ts"), "export const b = 1;\n").unwrap();
    std::fs::write(dir.join("B/c.ts"), "export const c = 1;\n").unwrap();
    std::fs::write(
        dir.join("A/zzop.config.jsonc"),
        r#"{ "trees": [{ "root": "./web" }, { "root": "./api" }] }"#,
    )
    .unwrap();
    std::fs::write(dir.join("B/zzop.config.jsonc"), r#"{ "roots": ["."] }"#).unwrap();

    let params = serde_json::json!({
        "name": "cross_repo",
        "arguments": { "paths": [dir.join("A").to_string_lossy(), dir.join("B").to_string_lossy()] }
    });
    let reply = super::call(Some(&params));
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(
        reply["isError"], true,
        "a path carrying its own tree set must refuse: {reply}"
    );
    let text = reply["content"][0]["text"].as_str().unwrap_or_default();
    assert!(
        text.contains(zzop_summary::contracts::PATHS_MODE_CONFIG_MARKER),
        "the refusal must be the shared paths-mode message, recognizable by its marker: {text}"
    );
    assert!(
        text.contains("configPath"),
        "the refusal must name the argument that answers on THIS host: {text}"
    );
    assert!(
        !text.contains("--config"),
        "CLI-only prescription leaked into the MCP wire: {text}"
    );
}

#[test]
fn multi_tree_refusal_reaching_an_mcp_client_names_this_host_s_tool_not_a_shell_line() {
    // The sibling of the test below, for the refusal the 2026-08-09 ruling was never applied to. The
    // shared string names the cross-layer join in PROSE and stays spelling-free (a CLI line is useless
    // to a shell-less client, and a tool name is useless in a terminal) -- so until 2026-09-23 neither
    // host spelled it, and both of the message's remedies failed when transcribed. This pins the MCP
    // half: the client must be told which TOOL answers, never a shell line.
    let dir = std::env::temp_dir().join(format!("zzop-mcp-multi-tree-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("web")).unwrap();
    std::fs::create_dir_all(dir.join("api")).unwrap();
    std::fs::write(dir.join("web/a.ts"), "export const a = 1;\n").unwrap();
    std::fs::write(dir.join("api/b.ts"), "export const b = 1;\n").unwrap();
    let cfg = dir.join("zzop.config.jsonc");
    std::fs::write(
        &cfg,
        r#"{ "trees": [{ "root": "./web" }, { "root": "./api" }] }"#,
    )
    .unwrap();

    let params = serde_json::json!({
        "name": "analyze_repo",
        "arguments": { "configPath": cfg.to_string_lossy() }
    });
    let reply = super::call(Some(&params));
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(
        reply["isError"], true,
        "a multi-tree config must refuse the single-tree lane: {reply}"
    );
    let text = reply["content"][0]["text"].as_str().unwrap_or_default();
    assert!(
        text.contains(zzop_summary::contracts::MULTI_TREE_MARKER),
        "the refusal must be the shared multi-tree message, recognizable by its marker: {text}"
    );
    assert!(
        text.contains("cross_repo"),
        "the refusal must name the tool that answers on THIS host: {text}"
    );
    // A shell line here would be advice this client cannot take -- the mirror of the assertion in the
    // test below, and the reason the shared string carries neither spelling.
    assert!(
        !text.contains("zzop cross --config"),
        "CLI-only prescription leaked into the MCP wire: {text}"
    );
}

#[test]
fn missing_config_refusal_reaching_an_mcp_client_carries_no_cli_only_prescription() {
    // The 2026-08-09 ruling split the missing-config answer in two: the SHARED string names only the
    // `config-template` artifact (host-neutral, guarded in crates/** by host_vocabulary contracts
    // 15/16), and each HOST appends its own way out at its own display layer — the CLI a
    // `Run `zzop init`` line, this server the orientation text's resource pointer. This test pins the
    // half no other test held: the refusal that actually crosses the MCP wire. Before it, packages/mcp
    // carried ZERO pins on this message, so a later handler wrapping the error into its own words
    // would have let the two hosts drift apart with CI green.
    let dir = std::env::temp_dir().join(format!("zzop-mcp-no-config-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let params = serde_json::json!({
        "name": "analyze_repo",
        "arguments": { "path": dir.to_string_lossy() }
    });
    let reply = super::call(Some(&params));
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(
        reply["isError"], true,
        "a config-less tree must refuse: {reply}"
    );
    let text = reply["content"][0]["text"].as_str().unwrap_or_default();
    assert!(
        text.contains(zzop_summary::contracts::MISSING_CONFIG_MARKER),
        "the refusal must be the shared missing-config message, recognizable by its marker: {text}"
    );
    assert!(
        text.contains("`config-template` contract document"),
        "the refusal must name the artifact this host can serve: {text}"
    );
    // The CLI's appended line must NOT ride the wire to a shell-less client. If this fires, the hint
    // leaked into the shared string — move it back to the CLI display layer (cli/mod.rs).
    assert!(
        !text.contains("zzop init"),
        "CLI-only prescription leaked into the MCP wire: {text}"
    );
    // 🔴 And the other half must BE there. The assertion above is one-sided: it forbids the CLI's
    // spelling without requiring this host's, and for a long time nothing required it — the MCP
    // refusal named the artifact and never said where to get it, so `resources/read` was reachable
    // only by an agent that already knew the URI space. An external review (2026-09-10, `4f5d05d0`)
    // measured it: URI present 0 times. A forbid-only pin lets a real gap sit green, which is this
    // repo's own headline failure shape — green means "what this check looks at is fine".
    let uri = format!(
        "{}{}",
        zzop_summary::contracts::URI_PREFIX,
        zzop_summary::contracts::CONFIG_TEMPLATE_NAME
    );
    assert!(
        text.contains(&uri),
        "the refusal must carry the address this host can actually serve ({uri}): {text}"
    );
}

/// 🔴 A `rule` filter that could not have matched anything is a FAILED call on THIS host too.
///
/// Measured 2026-09-01 on koel: `analyze_repo` with `rule: "definitely-not-a-rule"` returned a
/// 28,757-byte success with no `isError`, its reason at byte offset 28,406 — while `zzop analyze`
/// refused the identical argument with exit 2. Same binary family, same judgment available, opposite
/// answers, and the weaker one is the agent's surface.
///
/// CANARY, both directions: a REAL rule id through the same path must stay a clean success, or
/// "isError on a bad id" would be indistinguishable from "isError on everything".
#[test]
fn an_unmatchable_rule_filter_is_an_error_on_this_host_as_it_is_on_the_cli() {
    let dir = TempDir::new("zzop-mcp-rule-filter");
    dir.write("src/a.ts", "export const a = 1;\n");
    dir.write_starter_config();
    let path = dir.path().display().to_string();

    let bad = call_tool(
        "analyze_repo",
        serde_json::json!({ "path": path, "rule": "definitely-not-a-rule" }),
    );
    assert_eq!(
        bad["isError"], true,
        "an id that can be PROVEN to match no rule must not read as a clean run: {bad}"
    );

    // The reply itself stays `content[0]`, still parseable as the same JSON document the CLI twin
    // prints — the surface-parity contract is about the document, and an exit code is not part of it.
    let payload = bad["content"][0]["text"].as_str().expect("the reply text");
    let parsed: serde_json::Value =
        serde_json::from_str(payload).expect("content[0] must still be the analyze document");
    assert!(
        parsed["findings"]["total"].is_number(),
        "the analysis must still be delivered, exactly as the CLI delivers it on stdout while exiting \
         2: {payload}"
    );

    // And the REASON is one glance away rather than 28KB in.
    let reason = bad["content"][1]["text"]
        .as_str()
        .expect("the refusal must ride its own block");
    assert!(
        reason.contains("definitely-not-a-rule") && reason.contains("not a native analysis id"),
        "the second block must say which id and why it can never match: {reason}"
    );

    let good = call_tool(
        "analyze_repo",
        // The canary id must be one this build EVALUATES, not merely one it registers. It was
        // `dead-candidates` until 2026-09-05, and that id has SHIPPED OFF since 2026-09-03 — so the
        // canary was green only because the filter had no arm for "registered but never evaluated",
        // which is the very silence the rest of this test exists to refuse. A canary standing on a
        // defect asserts the defect: it went red the hour that arm landed, having vouched for
        // nothing in between.
        serde_json::json!({ "path": path, "rule": "circular" }),
    );
    std::fs::remove_dir_all(dir.path()).ok();
    assert!(
        good.get("isError").is_none(),
        "CANARY: a native analysis this run actually evaluated must stay a clean success, or the check above is a \
         statement about every rule filter rather than about unmatchable ones: {good}"
    );
    assert_eq!(
        good["content"].as_array().map(Vec::len),
        Some(1),
        "a clean call carries the document and nothing else: {good}"
    );
}

/// V226's landing pin. The module map existed — `zzop graph --domain dep --fold N` draws it — and
/// was reachable only from a terminal, while this product's named audience is an agent. Three
/// recorded reasons said no twin was needed; all three were measured false or half-false, which is
/// what opened this rather than any new want.
///
/// Asserts the properties the reply would be worthless without, not the whole document: `fold` is
/// REQUIRED here where the CLI twin defaults it, the fold really folded (two planted directories
/// become two modules with the import between them as ONE module edge), and the two honesty pairs
/// survive the MCP hop — `lines` never without `linesMeasuredOver`, and no truncation channel at all,
/// because nothing in this reply is capped.
#[test]
fn module_map_requires_its_grain_and_folds_the_graph_without_capping_anything() {
    let dir = TempDir::new("zzop-mcp-module-map");
    dir.write_starter_config();
    dir.write("lib/util.ts", "export const load = () => 1;\n");
    dir.write(
        "src/api.ts",
        "import { load } from '../lib/util';\nexport const go = () => load();\n",
    );
    let path = dir.path().to_string_lossy().to_string();

    // REQUIRED on the wire: an agent deciding how much of a tree to pull into a context window has
    // to say which grain it meant, where a terminal caller is looking at the answer already.
    let missing = call_tool("module_map", serde_json::json!({ "paths": [path.clone()] }));
    assert!(
        error_text(&missing).contains("missing `fold` argument"),
        "{missing}"
    );

    let reply = call_tool(
        "module_map",
        serde_json::json!({ "paths": [path], "fold": 1 }),
    );
    assert!(
        reply.get("isError").is_none(),
        "ONE path must be legal — a map is a question about a tree before it is about a join: {reply}"
    );
    let doc: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().expect("text")).unwrap();

    let ids: Vec<&str> = doc["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .map(|m| m["id"].as_str().expect("a module id is a string"))
        .collect();
    assert!(
        ids.contains(&"lib") && ids.contains(&"src"),
        "the fold must produce one module per planted directory: {ids:?}"
    );
    assert_eq!(doc["edges"][0]["from"], "src", "{doc}");
    assert_eq!(doc["edges"][0]["to"], "lib", "{doc}");
    assert_eq!(
        doc["edges"][0]["fileEdges"], 1,
        "one file-level import collapsed into this module edge: {doc}"
    );

    for m in doc["modules"].as_array().expect("modules array") {
        assert_eq!(
            m.get("lines").is_some(),
            m.get("linesMeasuredOver").is_some(),
            "`lines` is a SUM and never ships without the count of files it covers: {m}"
        );
    }
    // The absence IS the claim: this lane has no cap, so it has no channel to disclose one with.
    assert!(doc.get("truncated").is_none(), "{doc}");
    assert!(doc.get("filtered").is_none(), "{doc}");
    assert!(
        doc["meaning"].as_str().is_some_and(|m| m.len() > 200),
        "the reply carries its own legend: {doc}"
    );
}

/// C6's landing pin. `check_coverage` exists because the visibility surface — the one reply built to
/// say whether a zero from the four analyzing tools means anything — was reachable only from the CLI,
/// which locked the AGENT persona out of the disclosure lane while the parity registry recorded a
/// deferral ("promote if agent demand arrives") as though it were a judgment. The demand never
/// arrives spelled "show me that list"; it arrives as "analyze_repo said 0 — is that clean?".
///
/// Asserts the three things the tool would be worthless without, not the whole document (the facade
/// core's own suite owns the cells): a ONE-path call is legal here where `cross_repo` demands two,
/// the CAPABILITY rosters the CLI twin carries are on this wire too, and `unmeasured` — the field
/// that exists so recall cannot be dropped in transit — survives the MCP hop as a FIELD.
#[test]
fn check_coverage_answers_for_one_tree_and_carries_the_capability_and_unmeasured_axes() {
    let dir = TempDir::new("zzop-mcp-check-coverage");
    dir.write_starter_config();
    dir.write("src/a.ts", "export const a = 1;\n");
    let path = dir.path().to_string_lossy().to_string();

    let reply = call_tool("check_coverage", serde_json::json!({ "paths": [path] }));
    assert!(
        reply.get("isError").is_none(),
        "ONE path must be legal — the question is about a tree before it is about a join: {reply}"
    );
    let doc: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().expect("text")).unwrap();

    let trees = doc["trees"].as_array().expect("trees array");
    assert_eq!(trees.len(), 1, "{doc}");
    assert!(
        trees[0]["blindSpotBasis"].is_string(),
        "the sentence that keeps an empty `blindSpots` from reading as an all-clear must ride the \
         MCP hop too: {doc}"
    );

    // CAPABILITY axis: facts of the BUILD, true before any tree is walked. These are the half a
    // per-run reply can never carry, and the reason forwarding `coverage` inside analyze_repo was
    // not the same answer.
    assert!(
        doc["frameworkRecognizers"]
            .as_array()
            .is_some_and(|r| !r.is_empty()),
        "the compiled-in recognizer table is the build-capability half: {doc}"
    );
    assert!(
        doc["nativeAnalysesMeaning"].is_object() || doc["nativeAnalysesMeaning"].is_string(),
        "the native-analysis roster's legend is stated once at the root: {doc}"
    );

    // UNMEASURED axis: a FIELD, never a caveat sentence, precisely so it cannot be dropped in
    // transit the way prose is — and this hop is the transit that would have dropped it.
    let unmeasured = doc["unmeasured"].as_array().expect("unmeasured array");
    assert!(
        unmeasured.iter().any(|u| u["axis"] == "recall"),
        "recall must arrive as a field: {doc}"
    );
    // And the ruling it enforces: no single score, on this wire either.
    assert!(
        doc.get("score").is_none() && doc.get("coverageScore").is_none(),
        "by the 2026-07-31 ruling no folded score exists in this schema, host-independently: {doc}"
    );
    std::fs::remove_dir_all(dir.path()).ok();
}

/// The source-mode half: `paths` and `configPath` are exclusive, and the ERROR is the shared
/// handler's wording rather than a second sentence written for this arm — the same drift the
/// `cross_repo` arm's own comment records paying for.
#[test]
fn check_coverage_source_mode_errors_are_the_shared_handlers_verbatim() {
    let reply = call_tool("check_coverage", serde_json::json!({}));
    let text = error_text(&reply);
    assert!(
        text.contains("pass one tree root, 2+ tree roots, or a config file"),
        "this must be the SHARED front end's sentence, not one written for this arm — the CLI twin \
         answers a source-less `zzop coverage` with the same bytes: {text}"
    );
}
