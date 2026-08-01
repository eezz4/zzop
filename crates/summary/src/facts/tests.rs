//! Unit tests for the `zzop facts` projection — driven from LITERAL `analyzeTrees` outputs, with no
//! filesystem and no engine, so the shape contract is pinned independently of any corpus.

use super::project;

/// A minimal but realistic `analyzeTrees` output: one tree that provides a route, one that consumes a
/// different one, and the join buckets that follow.
///
/// The engine's own `ir` key is spelled straight out as a `json!` literal. It used to be grafted on
/// through a `const ENGINE_IR_FIELD: &str = "ir"` indirection, purely to dodge
/// `crates/engine/tests/rule_contracts/surface_parity.rs`'s TEST 3, which then treated any `"ir":` byte
/// in any host/summary/CLI source — a test fixture included — as an MCP leak. That test now scans
/// emission sources only (no tests, no comments, no CLI-only lanes), so the workaround is gone and the
/// fixture says what it means.
fn engine_output() -> serde_json::Value {
    serde_json::json!({
        "trees": [
            {
                "root": "/abs/api",
                "sourceId": "api",
                "output": {
                    "ir": {
                        "source": "api",
                        "parser": "engine",
                        "dep": { "src/app.ts": ["src/routes.ts"] },
                        "symbols": [{
                            "id": "src/routes.ts#listUsers", "file": "src/routes.ts",
                            "name": "listUsers", "kind": "function", "line": 4, "exported": true
                        }],
                        "loc": { "src/app.ts": 12, "src/routes.ts": 30 },
                        "io": {
                            "provides": [{
                                "kind": "http", "key": "GET /api/users",
                                "file": "src/routes.ts", "line": 4
                            }],
                            "consumes": []
                        }
                    },
                    "coverage": { "files": 2, "parserDispatched": 2, "joinContributionZero": false },
                    "warnings": ["a framework-silence self-report"],
                    "configWarnings": ["unknown rule id in overrides"]
                }
            },
            {
                "root": "/abs/web",
                "sourceId": "web",
                "output": {
                    "ir": {
                        "source": "web", "parser": "engine",
                        "dep": {}, "symbols": [], "loc": {}
                    },
                    "coverage": { "files": 1, "parserDispatched": 1, "joinContributionZero": true },
                    "warnings": [],
                    "configWarnings": []
                }
            }
        ],
        "crossLayer": {
            "edges": [],
            "unconsumedProvides": [{
                "source": "api", "kind": "http", "key": "GET /api/users",
                "file": "src/routes.ts", "line": 4
            }],
            "unprovidedConsumes": [],
            "unresolvedConsumes": [{
                "source": "web", "kind": "http", "key": null,
                "file": "src/client.ts", "line": 9, "raw": "`${BASE}/users`"
            }],
            "externalConsumes": [],
            "ambiguousConsumes": []
        },
        "crossLayerFindings": [{ "ruleId": "cross-layer/unconsumed-endpoint" }],
        "warnings": ["a run-level tripwire"],
        "disclosure": [{ "id": "capability-absent-vs-empty", "status": "asserted" }]
    })
}

fn projected() -> serde_json::Value {
    let text = project(&engine_output(), Some("/abs/zzop.config.jsonc"), Vec::new());
    serde_json::from_str(&text).expect("the projection emits valid JSON")
}

#[test]
fn the_top_level_key_set_is_pinned_exactly() {
    // Adding a key here is a wire-contract change for every rule program reading this surface — it
    // must be a deliberate edit to this list, never a side effect.
    let v = projected();
    let mut keys: Vec<&str> = v
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "config",
            "configWarnings",
            "crossLayer",
            "disclosure",
            "tool",
            "trees",
            "warnings"
        ]
    );
}

#[test]
fn every_key_is_present_even_when_the_run_produced_nothing() {
    // §0: a capability that can silently produce nothing must positively confirm it ran. An EMPTY
    // engine output must still yield the full document — never an absent key a reader could confuse
    // with either "zero" or "did not run".
    let text = project(&serde_json::json!({}), None, Vec::new());
    let v: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert!(v["tool"].is_string(), "the producing build is always named");
    assert!(v["config"].is_null(), "no config file is null, not absent");
    assert_eq!(v["configWarnings"], serde_json::json!([]));
    assert_eq!(v["trees"], serde_json::json!([]));
    assert_eq!(v["warnings"], serde_json::json!([]));
    assert_eq!(v["disclosure"], serde_json::json!([]));
    for bucket in [
        "edges",
        "unconsumedProvides",
        "unprovidedConsumes",
        "unresolvedConsumes",
        "externalConsumes",
        "ambiguousConsumes",
        "hostRekeyCounts",
    ] {
        assert_eq!(
            v["crossLayer"][bucket],
            serde_json::json!([]),
            "crossLayer.{bucket} must be materialized, not absent"
        );
    }
}

#[test]
fn a_tree_with_no_io_gets_a_materialized_empty_io_channel() {
    // `MinimalIr::io` is skip-if-none, so the `web` tree above carries no `io` key at all. A rule
    // author must not have to tell "the io channel is empty" from "the io channel is missing".
    let v = projected();
    let web = &v["trees"][1];
    assert_eq!(web["sourceId"], "web");
    assert_eq!(
        web["commonIr"]["io"],
        serde_json::json!({ "provides": [], "consumes": [] })
    );
    // ...and the rest of the IR is carried verbatim, not rebuilt.
    assert_eq!(web["commonIr"]["source"], "web");
    assert_eq!(web["commonIr"]["parser"], "engine");
}

#[test]
fn the_common_ir_is_carried_whole_not_projected_down_to_identity() {
    // The contrast with `zzop manifest`, which reads the same `ir` but keeps only (kind, key, source):
    // this surface carries dep/symbols/loc/io with their file and line intact, because a rule program
    // has to be able to report a location.
    let api = &projected()["trees"][0]["commonIr"];
    assert_eq!(api["dep"]["src/app.ts"][0], "src/routes.ts");
    assert_eq!(api["loc"]["src/routes.ts"], 30);
    assert_eq!(api["symbols"][0]["id"], "src/routes.ts#listUsers");
    assert_eq!(api["symbols"][0]["line"], 4);
    assert_eq!(api["io"]["provides"][0]["file"], "src/routes.ts");
    assert_eq!(api["io"]["provides"][0]["line"], 4);
}

#[test]
fn the_per_tree_honesty_channels_ride_alongside_the_facts() {
    let v = projected();
    assert_eq!(v["trees"][0]["coverage"]["joinContributionZero"], false);
    assert_eq!(
        v["trees"][0]["warnings"][0],
        "a framework-silence self-report"
    );
    // The blindness fact that makes every join verdict about this tree meaningless.
    assert_eq!(v["trees"][1]["coverage"]["joinContributionZero"], true);
    assert_eq!(v["trees"][1]["warnings"], serde_json::json!([]));
    // Run-level channels are distinct from the per-tree ones.
    assert_eq!(v["warnings"][0], "a run-level tripwire");
    assert_eq!(v["disclosure"][0]["id"], "capability-absent-vs-empty");
}

#[test]
fn tree_order_follows_the_request_not_a_re_sort() {
    // The crossLayer buckets are accumulated in tree order, so re-sorting only the tree array would
    // publish two contradictory orders in one document.
    let v = projected();
    assert_eq!(v["trees"][0]["sourceId"], "api");
    assert_eq!(v["trees"][1]["sourceId"], "web");
}

#[test]
fn zzops_own_findings_are_not_carried() {
    // Facts, not verdicts: a rule author computes their own. Carrying ours would put the same data on
    // two surfaces under two different caps.
    let v = projected();
    assert!(v.get("findings").is_none());
    assert!(v.get("crossLayerFindings").is_none());
    assert!(v["trees"][0].get("findings").is_none());
}

#[test]
fn the_unconsumed_endpoint_rule_is_reimplementable_from_the_emitted_facts() {
    // The adequacy test for this surface, executable rather than asserted in prose: re-derive
    // `cross-layer/unconsumed-endpoint`'s verdict for the fixture using ONLY emitted fields —
    // `crossLayer.unconsumedProvides` (kind/key/source/file/line), `crossLayer.unresolvedConsumes`
    // (the message's blindness count), and `crossLayer.edges` (from which the rule's
    // `trpc_participating_sources` exclusion is derived).
    let v = projected();
    let cl = &v["crossLayer"];
    let trpc_sources: Vec<&str> = cl["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|e| e["kind"] == "trpc")
        .flat_map(|e| [e["from"]["source"].as_str(), e["to"]["source"].as_str()])
        .flatten()
        .collect();
    let unresolved_http = cl["unresolvedConsumes"]
        .as_array()
        .expect("unresolvedConsumes")
        .iter()
        .filter(|c| c["kind"] == "http")
        .count();
    let reported: Vec<(String, u32)> = cl["unconsumedProvides"]
        .as_array()
        .expect("unconsumedProvides")
        .iter()
        .filter(|p| p["kind"] == "http")
        .filter(|p| !trpc_sources.contains(&p["source"].as_str().unwrap_or_default()))
        .filter(|p| !p["file"].as_str().unwrap_or_default().contains("/test"))
        .map(|p| {
            (
                p["file"].as_str().unwrap_or_default().to_string(),
                p["line"].as_u64().unwrap_or_default() as u32,
            )
        })
        .collect();
    assert_eq!(reported, [("src/routes.ts".to_string(), 4)]);
    assert_eq!(
        unresolved_http, 1,
        "the blindness count the real rule puts in its message is derivable too"
    );
}

#[test]
fn the_projection_is_byte_stable_for_the_same_input() {
    let a = project(&engine_output(), Some("/abs/zzop.config.jsonc"), Vec::new());
    let b = project(&engine_output(), Some("/abs/zzop.config.jsonc"), Vec::new());
    assert_eq!(a, b);
}

#[test]
fn config_warnings_are_forwarded_verbatim() {
    let text = project(
        &engine_output(),
        None,
        vec![serde_json::json!(
            "paths mode did not load a zzop.config.jsonc"
        )],
    );
    let v: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(
        v["configWarnings"],
        serde_json::json!(["paths mode did not load a zzop.config.jsonc"])
    );
}
