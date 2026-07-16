//! Tests for `query.rs` — the shared endpoint-query core. `query_io_json` is pure post-processing
//! over an analysis JSON string, so every fixture here is a handcrafted `analyzeTrees`-shaped (or
//! single-tree-shaped) value — no engine run, no filesystem.

use serde_json::{json, Value};

use crate::query::{query_io_json, QUERY_FINDINGS_LIMIT, QUERY_MATCH_LIMIT};

fn trees_output(cross_layer: Value, findings: Value, cross_layer_findings: Value) -> String {
    json!({
        "trees": [{
            "root": "/fe",
            "sourceId": "fe",
            "output": { "findings": findings, "warnings": [] }
        }],
        "crossLayer": cross_layer,
        "crossLayerFindings": cross_layer_findings,
        "disclosure": [{ "id": "fixture-class", "group": "extraction-blind", "summary": "s", "status": "asserted" }]
    })
    .to_string()
}

fn edge(key: &str) -> Value {
    json!({
        "kind": "http", "key": key,
        "from": { "source": "fe", "file": "src/api.ts", "line": 3 },
        "to": { "source": "be", "file": "src/users.controller.ts", "line": 7 },
        "crossSource": true
    })
}

fn consume(key: Option<&str>, raw: Option<&str>) -> Value {
    let mut v =
        json!({ "source": "fe", "kind": "http", "key": key, "file": "src/api.ts", "line": 9 });
    if let Some(raw) = raw {
        v["raw"] = json!(raw);
    }
    v
}

fn provide(key: &str) -> Value {
    json!({ "source": "be", "kind": "http", "key": key, "file": "src/app.ts", "line": 12 })
}

fn query(analysis: &str, pattern: &str) -> Value {
    let out = query_io_json(analysis, &json!({ "pattern": pattern }).to_string())
        .expect("query should succeed");
    serde_json::from_str(&out).unwrap()
}

#[test]
fn linked_verdict_from_a_matching_edge() {
    let analysis = trees_output(
        json!({ "edges": [edge("GET /api/users")], "unconsumedProvides": [], "unprovidedConsumes": [],
                "unresolvedConsumes": [], "externalConsumes": [], "ambiguousConsumes": [] }),
        json!([]),
        json!([]),
    );
    let v = query(&analysis, "users");
    assert_eq!(v["verdict"], "linked");
    assert_eq!(v["counts"]["edges"], 1);
    assert_eq!(v["counts"]["unprovidedConsumes"], 0);
    // Original objects intact: file/line/source survive.
    assert_eq!(v["matches"]["edges"][0]["from"]["file"], "src/api.ts");
    assert_eq!(v["matches"]["edges"][0]["key"], "GET /api/users");
    assert!(
        v.get("truncated").is_none(),
        "nothing capped => no truncated key"
    );
    assert!(
        v.get("suggestions").is_none(),
        "suggestions are not-found-only"
    );
}

#[test]
fn matching_is_case_insensitive_substring() {
    let analysis = trees_output(
        json!({ "edges": [edge("GET /api/Users")], "unconsumedProvides": [], "unprovidedConsumes": [],
                "unresolvedConsumes": [], "externalConsumes": [], "ambiguousConsumes": [] }),
        json!([]),
        json!([]),
    );
    assert_eq!(query(&analysis, "api/users")["verdict"], "linked");
    assert_eq!(query(&analysis, "USERS")["verdict"], "linked");
}

#[test]
fn each_single_bucket_class_maps_to_its_verdict_token() {
    let cases = [
        (
            "unconsumedProvides",
            provide("GET /api/users"),
            "provided-only",
        ),
        (
            "unprovidedConsumes",
            consume(Some("GET /api/users"), None),
            "consumed-unprovided",
        ),
        (
            "externalConsumes",
            consume(Some("GET https://vendor.com/api/users"), None),
            "external",
        ),
        (
            "ambiguousConsumes",
            consume(Some("GET /api/users"), None),
            "ambiguous",
        ),
    ];
    for (bucket, item, want) in cases {
        let mut cl = json!({ "edges": [], "unconsumedProvides": [], "unprovidedConsumes": [],
                             "unresolvedConsumes": [], "externalConsumes": [], "ambiguousConsumes": [] });
        cl[bucket] = json!([item]);
        let v = query(&trees_output(cl, json!([]), json!([])), "users");
        assert_eq!(v["verdict"], want, "bucket {bucket}");
        assert_eq!(v["counts"][bucket], 1, "bucket {bucket}");
    }
}

#[test]
fn unresolved_consume_matches_on_raw_when_key_is_null() {
    let cl = json!({ "edges": [], "unconsumedProvides": [], "unprovidedConsumes": [],
                     "unresolvedConsumes": [consume(None, Some("`${base}/api/users`")), consume(None, None)],
                     "externalConsumes": [], "ambiguousConsumes": [] });
    let v = query(&trees_output(cl, json!([]), json!([])), "users");
    assert_eq!(v["verdict"], "unresolved-only");
    // The raw-less consume has nothing to match on — it stays unmatched, never guessed.
    assert_eq!(v["counts"]["unresolvedConsumes"], 1);
}

#[test]
fn two_matching_classes_yield_mixed() {
    let cl = json!({ "edges": [edge("GET /api/users")], "unconsumedProvides": [],
                     "unprovidedConsumes": [consume(Some("POST /api/users"), None)],
                     "unresolvedConsumes": [], "externalConsumes": [], "ambiguousConsumes": [] });
    let v = query(&trees_output(cl, json!([]), json!([])), "users");
    assert_eq!(v["verdict"], "mixed");
    assert_eq!(v["counts"]["edges"], 1);
    assert_eq!(v["counts"]["unprovidedConsumes"], 1);
}

#[test]
fn match_lists_cap_with_disclosed_remainder_while_counts_stay_full() {
    let over = QUERY_MATCH_LIMIT + 5;
    let consumes: Vec<Value> = (0..over)
        .map(|i| consume(Some(&format!("GET /api/users/{i}")), None))
        .collect();
    let cl = json!({ "edges": [], "unconsumedProvides": [], "unprovidedConsumes": consumes,
                     "unresolvedConsumes": [], "externalConsumes": [], "ambiguousConsumes": [] });
    let v = query(&trees_output(cl, json!([]), json!([])), "users");
    assert_eq!(v["counts"]["unprovidedConsumes"], over);
    assert_eq!(
        v["matches"]["unprovidedConsumes"].as_array().unwrap().len(),
        QUERY_MATCH_LIMIT
    );
    assert_eq!(v["truncated"]["unprovidedConsumes"], 5);
    assert!(
        v["truncated"].get("edges").is_none(),
        "only capped buckets appear"
    );
}

#[test]
fn related_findings_come_from_tree_and_cross_layer_arrays_and_match_keys_too() {
    let cl = json!({ "edges": [edge("GET /api/users")], "unconsumedProvides": [], "unprovidedConsumes": [],
                     "unresolvedConsumes": [], "externalConsumes": [], "ambiguousConsumes": [] });
    let tree_findings = json!([
        { "ruleId": "a", "severity": "warning", "file": "f.ts", "line": 1,
          "message": "route GET /api/users has drift" },
        { "ruleId": "b", "severity": "info", "file": "g.ts", "line": 2, "message": "unrelated" }
    ]);
    let cl_findings = json!([
        { "ruleId": "cross-layer/route-near-miss", "severity": "info", "file": "h.ts", "line": 3,
          "message": "near miss on GET /API/USERS" }
    ]);
    // Pattern "api/users" matches the key "GET /api/users"; the cross-layer finding's message
    // contains the matched KEY (case-insensitively), not just the raw pattern.
    let v = query(&trees_output(cl, tree_findings, cl_findings), "api/users");
    let related = v["relatedFindings"].as_array().unwrap();
    assert_eq!(related.len(), 2);
    assert_eq!(related[0]["ruleId"], "a");
    assert_eq!(related[1]["ruleId"], "cross-layer/route-near-miss");
    assert!(v.get("truncatedFindings").is_none());
}

#[test]
fn related_findings_cap_discloses_remainder() {
    let over = QUERY_FINDINGS_LIMIT + 3;
    let findings: Vec<Value> = (0..over)
        .map(|i| {
            json!({ "ruleId": "r", "severity": "info", "file": "f.ts", "line": i,
                         "message": "mentions /api/users here" })
        })
        .collect();
    let cl = json!({ "edges": [edge("GET /api/users")], "unconsumedProvides": [], "unprovidedConsumes": [],
                     "unresolvedConsumes": [], "externalConsumes": [], "ambiguousConsumes": [] });
    let v = query(&trees_output(cl, json!(findings), json!([])), "users");
    assert_eq!(
        v["relatedFindings"].as_array().unwrap().len(),
        QUERY_FINDINGS_LIMIT
    );
    assert_eq!(v["truncatedFindings"], 3);
}

#[test]
fn not_found_suggests_keys_sharing_the_last_path_segment() {
    let cl = json!({ "edges": [edge("GET /api/v2/users")], "unconsumedProvides": [provide("GET /health")],
                     "unprovidedConsumes": [], "unresolvedConsumes": [], "externalConsumes": [],
                     "ambiguousConsumes": [] });
    let v = query(&trees_output(cl, json!([]), json!([])), "/internal/users");
    assert_eq!(v["verdict"], "not-found");
    assert_eq!(v["suggestions"], json!(["GET /api/v2/users"]));
}

#[test]
fn not_found_falls_back_to_any_pattern_segment_containment() {
    let cl = json!({ "edges": [edge("GET /api/user-detail")], "unconsumedProvides": [],
                     "unprovidedConsumes": [], "unresolvedConsumes": [], "externalConsumes": [],
                     "ambiguousConsumes": [] });
    // The pattern's last segment ("list") equals no key's last segment, so the primary rule yields
    // nothing; the fallback keeps any key containing SOME pattern segment — "api" here.
    let v = query(&trees_output(cl, json!([]), json!([])), "api/list");
    assert_eq!(v["verdict"], "not-found");
    assert_eq!(v["suggestions"], json!(["GET /api/user-detail"]));
}

#[test]
fn disclosure_is_forwarded_verbatim() {
    let cl = json!({ "edges": [], "unconsumedProvides": [], "unprovidedConsumes": [],
                     "unresolvedConsumes": [], "externalConsumes": [], "ambiguousConsumes": [] });
    let v = query(&trees_output(cl, json!([]), json!([])), "anything");
    assert_eq!(v["disclosure"][0]["id"], "fixture-class");
}

#[test]
fn empty_or_missing_pattern_is_an_error() {
    let analysis = trees_output(
        json!({ "edges": [], "unconsumedProvides": [], "unprovidedConsumes": [],
                "unresolvedConsumes": [], "externalConsumes": [], "ambiguousConsumes": [] }),
        json!([]),
        json!([]),
    );
    for bad in [json!({}), json!({ "pattern": "" }), json!({ "pattern": 3 })] {
        let err = query_io_json(&analysis, &bad.to_string()).unwrap_err();
        assert!(
            err.contains("pattern"),
            "error should name the pattern field: {err}"
        );
    }
}

#[test]
fn unknown_query_key_is_a_named_error() {
    let analysis = trees_output(
        json!({ "edges": [], "unconsumedProvides": [], "unprovidedConsumes": [],
                "unresolvedConsumes": [], "externalConsumes": [], "ambiguousConsumes": [] }),
        json!([]),
        json!([]),
    );
    let err = query_io_json(&analysis, r#"{"pattern": "x", "patern": "typo"}"#).unwrap_err();
    assert!(
        err.contains("patern"),
        "error should name the unknown key: {err}"
    );
}

#[test]
fn single_tree_output_is_a_guided_error_with_raw_io_counts() {
    let single = json!({
        "ir": { "source": "app", "parser": "typescript",
                "io": { "provides": [{ "kind": "http", "key": "GET /api/users", "file": "a.ts", "line": 1 }],
                        "consumes": [] } },
        "findings": [],
        "disclosure": []
    })
    .to_string();
    let err = query_io_json(&single, r#"{"pattern": "users"}"#).unwrap_err();
    assert!(
        err.contains("analyzeTrees"),
        "guided at a trees analysis: {err}"
    );
    assert!(
        err.contains("1 provides"),
        "reports the raw match counts: {err}"
    );
}

#[test]
fn non_analysis_json_is_a_named_error() {
    let err = query_io_json(r#"{"hello": 1}"#, r#"{"pattern": "x"}"#).unwrap_err();
    assert!(err.contains("not a zzop analysis output"), "{err}");
}
