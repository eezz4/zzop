use super::*;
use serde_json::Value;

/// The same shape `dep/tests.rs` uses — a 3-file cycle plus an unrelated leaf — so a reader comparing
/// the two formats is comparing them over identical input rather than two fixtures that might differ.
fn one_tree() -> Value {
    serde_json::json!({
        "trees": [{
            "sourceId": "web",
            "output": {
                "ir": { "dep": {
                    "src/a.ts": ["src/b.ts"],
                    "src/b.ts": ["src/c.ts"],
                    "src/c.ts": ["src/a.ts"],
                    "src/leaf.ts": []
                }},
                "findings": [{
                    "ruleId": "circular", "severity": "warning", "file": "src/a.ts", "line": 1,
                    "message": "m", "data": { "members": ["src/a.ts", "src/b.ts", "src/c.ts"] }
                }]
            }
        }]
    })
}

fn rows(ndjson: &str) -> Vec<Value> {
    ndjson
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("every emitted line is valid JSON"))
        .collect()
}

#[test]
fn every_line_is_a_standalone_json_object() {
    let u = super::super::dep::collect(&one_tree());
    let (nodes, _) = nodes_ndjson(&u, None);
    let (links, _) = links_ndjson(&u, None);
    // The parse in `rows` IS the assertion — NDJSON's whole contract is that a consumer can read one
    // line at a time without a streaming parser.
    assert_eq!(rows(&nodes).len(), 4);
    assert_eq!(rows(&links).len(), 3);
}

/// The property that separates this lane from the mermaid one. `dep`'s cap would drop nodes here; this
/// lane must not, and the census must say so rather than leaving the reader to assume it.
#[test]
fn the_lane_is_uncapped_and_says_so() {
    let u = super::super::dep::collect(&one_tree());
    let (nodes, census) = nodes_ndjson(&u, None);
    assert_eq!(rows(&nodes).len(), u.nodes.len());
    assert_eq!(census.nodes_emitted, 4);
    assert_eq!(census.total_nodes, 4);
    assert!(census.render().contains("UNCAPPED"), "{}", census.render());
}

#[test]
fn a_node_carries_every_axis_a_viewer_can_style_by() {
    let u = super::super::dep::collect(&one_tree());
    let (nodes, _) = nodes_ndjson(&u, None);
    let a = rows(&nodes)
        .into_iter()
        .find(|r| r["id"] == "src/a.ts")
        .expect("src/a.ts is a node");
    assert_eq!(a["source"], "web");
    assert_eq!(a["path"], "src/a.ts");
    // A basename, not the path — this is the one zzop surface whose reader is a person looking at a
    // screen, and a full path on every node is unreadable at graph scale.
    assert_eq!(a["label"], "a.ts");
    assert_eq!(a["folder"], "src");
    assert_eq!(a["inCycle"], true);
    // a -> b, and c -> a.
    assert_eq!(a["fanIn"], 1);
    assert_eq!(a["fanOut"], 1);
    assert_eq!(a["degree"], 2);
}

/// Cycle membership is the highest-severity structural fact this domain carries. The mermaid lane draws
/// it as a thick arrow; a viewer has no arrow styles, so it has to survive as a COLUMN or it is lost.
#[test]
fn cycle_membership_survives_into_both_tables() {
    let u = super::super::dep::collect(&one_tree());
    let (nodes, _) = nodes_ndjson(&u, None);
    let leaf = rows(&nodes)
        .into_iter()
        .find(|r| r["id"] == "src/leaf.ts")
        .expect("leaf is a node");
    assert_eq!(leaf["inCycle"], false);

    let (links, _) = links_ndjson(&u, None);
    assert!(
        rows(&links).iter().all(|r| r["inCycle"] == true),
        "all three edges are inside the 3-file cycle: {links}"
    );
}

/// An edge pointing at a node the table does not contain is a dangling reference in the viewer. The
/// mermaid lane states this rule in its note; here it has to actually hold across two files.
#[test]
fn scope_drops_edges_whose_other_end_it_dropped() {
    let tree = serde_json::json!({
        "trees": [{
            "sourceId": "web",
            "output": {
                "ir": { "dep": { "keep/a.ts": ["drop/b.ts"], "keep/c.ts": ["keep/a.ts"] }},
                "findings": []
            }
        }]
    });
    let u = super::super::dep::collect(&tree);
    let (nodes, census) = nodes_ndjson(&u, Some("keep/"));
    let ids: Vec<String> = rows(&nodes)
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, vec!["keep/a.ts", "keep/c.ts"]);

    let (links, _) = links_ndjson(&u, Some("keep/"));
    let links = rows(&links);
    assert_eq!(links.len(), 1, "only the wholly-in-scope edge survives");
    assert_eq!(links[0]["source"], "keep/c.ts");
    assert_eq!(links[0]["target"], "keep/a.ts");
    // The drop has to be REPORTED, not merely correct.
    assert!(
        census.render().contains("--scope dropped"),
        "{}",
        census.render()
    );
}

/// Byte-stability is the same contract every other zzop surface carries; a viewer that reloads a file
/// must not see a different graph because a `HashMap` iterated differently.
#[test]
fn output_is_byte_stable_across_runs() {
    let u = super::super::dep::collect(&one_tree());
    for _ in 0..3 {
        let (n, _) = nodes_ndjson(&u, None);
        let (l, _) = links_ndjson(&u, None);
        let (n2, _) = nodes_ndjson(&super::super::dep::collect(&one_tree()), None);
        let (l2, _) = links_ndjson(&super::super::dep::collect(&one_tree()), None);
        assert_eq!(n, n2);
        assert_eq!(l, l2);
    }
}

/// A path containing a quote or a comma is exactly the case a hand-rolled CSV writer gets wrong; the
/// reason this lane is NDJSON is that `serde_json` owns the escaping. Pinned so a future "let's emit CSV
/// instead" arrives with this test in front of it.
#[test]
fn punctuation_in_a_path_cannot_corrupt_a_row() {
    let tree = serde_json::json!({
        "trees": [{
            "sourceId": "web",
            "output": {
                "ir": { "dep": { "src/we\"ird, name.ts": ["src/b.ts"] }},
                "findings": []
            }
        }]
    });
    let u = super::super::dep::collect(&tree);
    let (nodes, _) = nodes_ndjson(&u, None);
    let weird = rows(&nodes)
        .into_iter()
        .find(|r| r["label"] == "we\"ird, name.ts")
        .expect("the awkward name round-trips through one line");
    assert_eq!(weird["folder"], "src");
}
