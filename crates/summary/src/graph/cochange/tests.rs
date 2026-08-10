use super::*;
use crate::graph::fold::Fold;
use serde_json::json;

fn tree(source: &str, co_change: Value) -> Value {
    json!({"sourceId": source, "output": {"coChange": co_change}})
}

fn edge(a: &str, b: &str, count: u64) -> Value {
    json!({"a": a, "b": b, "count": count})
}

#[test]
fn draws_undirected_edges_labelled_with_the_commit_count() {
    let v = json!({"trees": [tree("x", json!([edge("a.rs", "b.rs", 7)]))]});
    let out = project(&v, None, 10, Fold::of(None));
    assert!(out.contains("--- |7|"), "{out}");
    assert!(out.contains("\"a.rs\""), "{out}");
    assert!(!out.contains("-->"), "co-change has no direction: {out}");
}

#[test]
fn a_null_field_is_reported_as_not_measured_not_as_an_empty_result() {
    // The one error this domain must not make. `null` means git never ran there.
    let v = json!({"trees": [tree("x", Value::Null)]});
    let out = project(&v, None, 10, Fold::of(None));
    assert!(out.contains("NOT MEASURED in 1 of 1 tree(s)"), "{out}");
    assert!(
        !out.contains("MEASURED AND EMPTY"),
        "an unmeasured tree must not be reported as a measured zero: {out}"
    );
}

#[test]
fn an_empty_array_is_reported_as_a_measured_zero() {
    let v = json!({"trees": [tree("x", json!([]))]});
    let out = project(&v, None, 10, Fold::of(None));
    assert!(out.contains("MEASURED AND EMPTY"), "{out}");
    assert!(!out.contains("NOT MEASURED"), "{out}");
}

#[test]
fn the_sample_disclosure_is_always_present_even_on_a_full_picture() {
    let v = json!({"trees": [tree("x", json!([edge("a.rs", "b.rs", 3)]))]});
    let out = project(&v, None, 10, Fold::of(None));
    assert!(out.contains("A SAMPLE, never a repository total"), "{out}");
    assert!(out.contains("NOT imports"), "{out}");
}

#[test]
fn the_cap_is_disclosed_with_both_the_scoped_and_total_counts() {
    let edges: Vec<Value> = (0..5)
        .map(|i| edge(&format!("a{i}.rs"), &format!("b{i}.rs"), 5 - i))
        .collect();
    let v = json!({"trees": [tree("x", Value::Array(edges))]});
    let out = project(&v, None, 2, Fold::of(None));
    assert!(out.contains("drawn 2 / in-scope 5 / total 5"), "{out}");
}

#[test]
fn scope_keeps_a_pair_when_either_endpoint_is_inside_it() {
    // A tie reaching OUT of the scoped area is what a reader scoping to a folder wants to see.
    let v = json!({"trees": [tree("x", json!([edge("src/a.rs", "docs/b.md", 4)]))]});
    let out = project(&v, Some("src/"), 10, Fold::of(None));
    assert!(out.contains("docs/b.md"), "{out}");
    assert!(out.contains("in-scope 1"), "{out}");
}

#[test]
fn engine_order_is_preserved_rather_than_re_sorted() {
    // The engine ranks by count desc; truncating (not re-sorting) is what keeps this picture agreeing
    // with every other surface about which tie is strongest.
    let v =
        json!({"trees": [tree("x", json!([edge("a.rs", "b.rs", 9), edge("c.rs", "d.rs", 1)]))]});
    let out = project(&v, None, 1, Fold::of(None));
    assert!(out.contains("a.rs"), "{out}");
    assert!(!out.contains("c.rs"), "{out}");
}

// --- `--fold` ------------------------------------------------------------------------------------

/// A tree that also declares which files the RUN analyzed, so the history-vs-working-tree census below
/// has something to measure against. `tree` above declares none, which is itself the interesting case.
fn tree_with_analyzed(source: &str, co_change: Value, analyzed: &[&str]) -> Value {
    let mut loc = serde_json::Map::new();
    for f in analyzed {
        loc.insert((*f).to_string(), json!(1));
    }
    json!({"sourceId": source, "output": {"coChange": co_change, "ir": {"loc": loc}}})
}

#[test]
fn folding_collapses_pairs_and_relabels_the_edge_with_the_pair_count() {
    let v = json!({"trees": [tree("x", json!([
        edge("fe/a.ts", "be/x.ts", 9),
        edge("fe/b.ts", "be/y.ts", 4),
    ]))]});
    let out = project(&v, None, 10, Fold::of(Some(1)));
    // Two file pairs between the same two boxes become ONE edge labelled 2 — never 13, which is what
    // summing the commit counts would have produced and would exceed the commits that exist.
    assert!(out.contains("--- |2|"), "{out}");
    assert!(
        !out.contains("|13|"),
        "commit counts must not be summed across pairs: {out}"
    );
    assert!(out.contains("FOLDED to --fold 1"), "{out}");
    assert!(out.contains("NUMBER OF FILE-LEVEL EDGES"), "{out}");
}

#[test]
fn a_pair_inside_one_box_is_dropped_and_the_picture_says_why() {
    let v = json!({"trees": [tree("x", json!([
        edge("fe/a.ts", "fe/b.ts", 9),
        edge("fe/c.ts", "be/x.ts", 3),
    ]))]});
    let out = project(&v, None, 10, Fold::of(Some(1)));
    assert!(out.contains("internal cohesion"), "{out}");
    // One drawn edge, not two: the fe/fe tie folded into the box and left.
    assert_eq!(out.matches(" --- ").count(), 1, "{out}");
}

#[test]
fn the_cap_is_reported_against_the_folded_count_not_the_file_count() {
    let v = json!({"trees": [tree("x", json!([
        edge("a/1.ts", "b/1.ts", 9),
        edge("c/1.ts", "d/1.ts", 8),
        edge("e/1.ts", "f/1.ts", 7),
    ]))]});
    let out = project(&v, None, 2, Fold::of(Some(1)));
    assert!(
        out.contains("the cap applies AFTER the fold: 2 of 3"),
        "{out}"
    );
}

#[test]
fn an_unfolded_picture_carries_no_fold_disclosure_at_all() {
    let v = json!({"trees": [tree("x", json!([edge("fe/a.ts", "be/x.ts", 2)]))]});
    let out = project(&v, None, 10, Fold::of(None));
    assert!(!out.contains("FOLDED"), "{out}");
    assert!(!out.contains("internal cohesion"), "{out}");
    // And no `|n|` relabelling: the file-level label is still the commit count.
    assert!(out.contains("--- |2|"), "{out}");
}

#[test]
fn a_path_git_remembers_but_the_run_did_not_analyze_is_counted_not_dropped() {
    let v = json!({"trees": [tree_with_analyzed(
        "x",
        json!([edge("live.rs", "deleted/gone.rs", 5)]),
        &["live.rs"],
    )]});
    let out = project(&v, None, 10, Fold::of(None));
    assert!(
        out.contains("1 of 2 path(s) here are NOT in this run's analyzed file set"),
        "{out}"
    );
    // Never dropped — a module that was deleted and used to pull half the repo is the point.
    assert!(out.contains("deleted/gone.rs"), "{out}");
}

#[test]
fn a_picture_whose_paths_were_all_analyzed_carries_no_history_gap_line() {
    let v = json!({"trees": [tree_with_analyzed(
        "x",
        json!([edge("a.rs", "b.rs", 5)]),
        &["a.rs", "b.rs"],
    )]});
    let out = project(&v, None, 10, Fold::of(None));
    assert!(
        !out.contains("analyzed file set"),
        "an always-on caveat teaches nothing: {out}"
    );
}
