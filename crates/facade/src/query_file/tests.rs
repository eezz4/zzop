use super::*;

/// One analyzeTrees-shaped fixture covering all four verdicts at once, so a change that fixes one and
/// breaks another cannot pass.
fn analysis() -> String {
    json!({
        "trees": [{
            "sourceId": "web",
            "output": {
            "degraded": ["src/broken.ts"],
            "findings": [
                { "ruleId": "circular", "severity": "warning", "file": "src/api.ts", "line": 3, "message": "m" },
                { "ruleId": "db/x", "severity": "info", "file": "src/api.ts", "line": 9, "message": "m" },
                { "ruleId": "db/x", "severity": "info", "file": "src/other.ts", "line": 1, "message": "m" }
            ],
            "ir": {
                "loc": { "src/api.ts": 42, "src/other.ts": 7, "src/broken.ts": 5, "docs/notes.md": 3 },
                "dep": { "src/api.ts": ["src/other.ts"], "src/other.ts": [] },
                "symbols": [
                    { "id": "src/api.ts#load", "file": "src/api.ts", "name": "load", "exported": true },
                    { "id": "src/api.ts#helper", "file": "src/api.ts", "name": "helper", "exported": false }
                ],
                "io": {
                    "provides": [{ "kind": "http", "key": "GET /users", "file": "src/api.ts", "line": 3 }],
                    "consumes": [{ "kind": "http", "key": "GET /orders", "file": "src/other.ts", "line": 1 }]
                }
            }
            }
        }],
        "crossLayerFindings": [
            { "ruleId": "cross-layer/y", "severity": "critical", "file": "src/api.ts", "line": 3, "message": "m" }
        ]
    })
    .to_string()
}

fn q(path: &str) -> Value {
    let out =
        query_file_json(&analysis(), &json!({ "path": path }).to_string()).expect("query runs");
    serde_json::from_str(&out).expect("reply is JSON")
}

#[test]
fn an_analyzed_file_reports_its_projection_and_says_an_empty_list_would_mean_clean() {
    let v = q("src/api.ts");
    assert_eq!(v["verdict"], "analyzed");
    assert_eq!(v["sourceId"], "web");
    assert_eq!(v["loc"], 42);
    assert_eq!(v["symbols"]["count"], 2);
    assert_eq!(v["symbols"]["exported"], json!(["load"]));
    assert!(
        v["verdictMeaning"]
            .as_str()
            .unwrap()
            .contains("means clean"),
        "the token must carry its own meaning: {}",
        v["verdictMeaning"]
    );
}

/// The distinction this whole surface exists for: a file nothing structural ran on must NOT read as
/// clean. If this regresses, a caller asking about a `.md`/`.rb`/`.kt` file gets silence and believes it.
#[test]
fn a_lexical_only_file_says_out_loud_that_silence_is_not_an_all_clear() {
    let v = q("docs/notes.md");
    assert_eq!(v["verdict"], "lexical-only");
    let meaning = v["verdictMeaning"].as_str().unwrap();
    assert!(meaning.contains("does NOT mean clean"), "{meaning}");
    assert!(meaning.contains("parsers.globOverrides"), "{meaning}");
    assert_eq!(v["findings"]["total"], 0);
}

#[test]
fn a_degraded_file_is_reported_as_degraded_not_as_lexical_only() {
    let v = q("src/broken.ts");
    assert_eq!(v["verdict"], "degraded");
    assert!(v["verdictMeaning"]
        .as_str()
        .unwrap()
        .contains("does NOT mean clean"));
}

#[test]
fn a_path_this_run_never_walked_is_not_found_and_suggests_the_nearest_walked_paths() {
    let v = q("src/api.tsx");
    assert_eq!(v["verdict"], "not-found");
    let s = v["suggestions"].as_array().unwrap();
    assert!(
        s.iter().any(|x| x == "src/api.ts"),
        "the near-miss must be offered: {s:?}"
    );
}

/// Findings are the file's own PLUS any cross-layer finding anchored there — a caller asking about one
/// file should not have to know that the join reports through a different array.
#[test]
fn findings_merge_the_trees_own_and_the_cross_layer_ones_anchored_here() {
    let v = q("src/api.ts");
    assert_eq!(v["findings"]["total"], 3);
    assert_eq!(v["findings"]["bySeverity"]["critical"], 1);
    assert_eq!(v["findings"]["byRule"]["db/x"], 1);
    let ids: Vec<&str> = v["findings"]["list"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["ruleId"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"cross-layer/y"), "{ids:?}");
    // ...and nothing anchored in a DIFFERENT file leaks in.
    assert!(!ids.contains(&"db/x") || v["findings"]["byRule"]["db/x"] == 1);
}

/// `importedBy` is the half a caller cannot compute from the file's own text.
#[test]
fn dependencies_answer_both_directions() {
    let v = q("src/other.ts");
    assert_eq!(v["dependencies"]["imports"], json!([]));
    assert_eq!(v["dependencies"]["importedBy"], json!(["src/api.ts"]));
}

#[test]
fn io_facts_are_scoped_to_the_target_file() {
    let v = q("src/api.ts");
    assert_eq!(v["io"]["provides"].as_array().unwrap().len(), 1);
    assert_eq!(
        v["io"]["consumes"].as_array().unwrap().len(),
        0,
        "the consume belongs to src/other.ts"
    );
}

/// An agent usually holds an ABSOLUTE path. Accepting the tail is what makes this surface usable without
/// making the caller compute tree-relative paths first.
#[test]
fn an_absolute_looking_path_matches_by_its_tail() {
    let v = q("/home/me/proj/src/api.ts");
    assert_eq!(v["verdict"], "analyzed");
    assert_eq!(v["target"], "src/api.ts", "the reply reports the REL path");
}

#[test]
fn a_windows_separator_path_matches_too() {
    let v = q("src\\api.ts");
    assert_eq!(v["verdict"], "analyzed");
}

/// A single-tree `analyze` output has no tree identity, so the query refuses rather than inventing one —
/// the same shape `query_io` uses for a pre-join output.
#[test]
fn a_non_trees_analysis_is_a_guided_error_not_a_wrong_answer() {
    let err = query_file_json(r#"{"ir":{},"findings":[]}"#, r#"{"path":"a.ts"}"#).unwrap_err();
    assert!(err.contains("analyzeTrees"), "{err}");
}

#[test]
fn a_query_without_a_path_is_an_error() {
    assert!(query_file_json(&analysis(), "{}").is_err());
}

/// The same relative path in two trees must never be answered silently from one of them.
#[test]
fn a_path_present_in_two_trees_names_the_others_rather_than_picking_quietly() {
    let two = json!({
        "trees": [
            { "sourceId": "a", "output": { "ir": { "loc": { "src/x.ts": 1 }, "dep": { "src/x.ts": [] }, "symbols": [], "io": {} } } },
            { "sourceId": "b", "output": { "ir": { "loc": { "src/x.ts": 2 }, "dep": { "src/x.ts": [] }, "symbols": [], "io": {} } } }
        ]
    })
    .to_string();
    let out = query_file_json(&two, r#"{"path":"src/x.ts"}"#).unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["sourceId"], "a");
    assert_eq!(v["otherTrees"], json!(["b"]));
    // ...and naming the tree pins the answer to it.
    let out_b = query_file_json(&two, r#"{"path":"src/x.ts","sourceId":"b"}"#).unwrap();
    let vb: Value = serde_json::from_str(&out_b).unwrap();
    assert_eq!(vb["sourceId"], "b");
    assert_eq!(vb["loc"], 2);
    assert!(vb.get("otherTrees").is_none());
}

/// Every token the code can emit must have a real meaning string, and the sealed list must match — a
/// token added to one and not the other is exactly how a wire vocabulary rots.
#[test]
fn every_sealed_verdict_token_carries_a_meaning() {
    for token in FILE_VERDICTS {
        assert_ne!(
            verdict_meaning(token),
            "unknown verdict token",
            "{token} is in the sealed list but has no meaning"
        );
    }
    assert_eq!(FILE_VERDICTS.len(), 4);
}
