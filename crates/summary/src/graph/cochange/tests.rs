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

/// The census (`%%`) does not survive into a rendered picture, so a truncated document must ALSO carry
/// a visible note node — the tier-wide promise `graph --help` makes ("a %% census plus a visible note
/// node"). Until 2026-08-15 cochange was the one domain of five with no note path: a default run over
/// this repo drew 30 of 747 pairs and the rendered picture said nothing.
#[test]
fn a_truncated_picture_carries_a_visible_note_node_not_just_the_census_comment() {
    let edges: Vec<Value> = (0..5)
        .map(|i| edge(&format!("a{i}.rs"), &format!("b{i}.rs"), 5 - i))
        .collect();
    let v = json!({"trees": [tree("x", Value::Array(edges))]});
    let out = project(&v, None, 2, Fold::of(None));
    assert!(
        out.contains(
            "zzopNote[\"PARTIAL VIEW: 2 of 5 co-change pair(s) drawn (3 dropped by --top 2)"
        ),
        "{out}"
    );
    // The pointer must NEVER name `zzop facts` — that lane emits `commonIr` (= output.ir) and
    // coChange is ir's sibling, absent from facts since the lane was born. risk.rs:175 records the
    // harm the last time a note here pointed at a lane without the data.
    assert!(!out.contains("zzop facts"), "{out}");
}

/// `graph --help` promises a visible note for every cap/filter — --scope included. The `%%` census
/// line does not survive rendering, so a scoped picture must carry its own node (the join lane's
/// dedicated SCOPED node is the sibling precedent).
#[test]
fn a_scoped_picture_carries_a_visible_scope_node() {
    let v = json!({"trees": [tree("x", json!([
        edge("src/a.rs", "src/b.rs", 4),
        edge("docs/c.md", "docs/d.md", 3),
    ]))]});
    let out = project(&v, Some("src/"), 10, Fold::of(None));
    assert!(
        out.contains("zzopScope[\"SCOPED to 'src/': 1 of 2 co-change pair(s)"),
        "{out}"
    );
    // And an UNSCOPED picture carries none — the node means "a filter removed something you cannot see".
    let full = project(&v, None, 10, Fold::of(None));
    assert!(!full.contains("zzopScope"), "{full}");
}

/// Same promise for --fold: the rendered canvas must say its nodes are boxes, not files — the `%%`
/// fold census is invisible once drawn.
#[test]
fn a_folded_picture_carries_a_visible_fold_node() {
    let v = json!({"trees": [tree("x", json!([edge("fe/a.ts", "be/x.ts", 9)]))]});
    let out = project(&v, None, 10, Fold::of(Some(1)));
    assert!(
        out.contains("zzopFold[\"FOLDED: a node is a path-prefix box"),
        "{out}"
    );
    let unfolded = project(&v, None, 10, Fold::of(None));
    assert!(!unfolded.contains("zzopFold"), "{unfolded}");
}

/// The complement that keeps the note meaningful: a full picture stays note-free (posture's pattern —
/// emit only when something was dropped, unlike dep/risk which also stamp a "complete" note).
#[test]
fn a_full_picture_carries_no_partial_view_note() {
    let v = json!({"trees": [tree("x", json!([edge("a.rs", "b.rs", 3)]))]});
    let out = project(&v, None, 10, Fold::of(None));
    assert!(!out.contains("zzopNote"), "{out}");
}

/// Under a fold the cap applies AFTER folding, so the note must count folded edges — the same
/// population the census line counts — not raw pairs (quoting raw pairs would contradict the census
/// two lines above it).
#[test]
fn under_a_fold_the_note_counts_folded_edges_like_the_census_does() {
    // 4 pairs across distinct top-level dirs so a --fold 1 keeps them as 4 distinct folded edges.
    let edges: Vec<Value> = (0..4)
        .map(|i| edge(&format!("d{i}/a.rs"), &format!("e{i}/b.rs"), 4 - i))
        .collect();
    let v = json!({"trees": [tree("x", Value::Array(edges))]});
    let out = project(&v, None, 2, Fold::of(Some(1)));
    assert!(
        out.contains("zzopNote[\"PARTIAL VIEW: 2 of 4 folded edge(s) drawn (2 dropped by --top 2)"),
        "{out}"
    );
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

/// T2 value pin, same shape as the posture-verbs and cosmograph-schema dev-dep pins: the co-change
/// commit window and per-file partner cap are OWNED by `zzop_metrics::coupling`
/// (`MIN_FILES_PER_COMMIT` / `MAX_FILES_PER_COMMIT` / `COUPLING_TOP_PER_FILE`), but the layering
/// forbids this crate a shipped dependency on `zzop-metrics`, so the shipped sample disclosure and
/// the two documents that restate the window hold the values as prose literals. Every needle below
/// is DERIVED from the owning constants and counted, not just found: two of the doc phrases occur
/// twice, and a `contains` would stay green while one of the pair went stale. Tune a constant and
/// each un-updated sentence fails here by name — the "typed 17" drift class (`risk.rs:14`) that
/// otherwise goes quietly false.
#[test]
fn the_commit_window_prose_in_the_disclosure_and_both_documents_matches_the_owning_constants() {
    use zzop_metrics::{COUPLING_TOP_PER_FILE, MAX_FILES_PER_COMMIT, MIN_FILES_PER_COMMIT};

    // The shipped sentence, asserted in ACTUAL rendered output rather than in this crate's source.
    let v = json!({"trees": [tree("x", json!([edge("a.rs", "b.rs", 3)]))]});
    let out = project(&v, None, 10, Fold::of(None));
    let shipped =
        format!("fewer than {MIN_FILES_PER_COMMIT} or more than {MAX_FILES_PER_COMMIT} files");
    assert!(
        out.contains(&shipped),
        "the shipped sample disclosure no longer states the owning window ({shipped}): {out}"
    );

    let count = |haystack: &str, needle: &str, n: usize, file: &str| {
        let found = haystack.matches(needle).count();
        assert_eq!(
            found, n,
            "{file} states the co-change window as {needle:?} {found} time(s), expected {n} — \
             either the owning constants moved and a sentence went stale, or a mention was \
             added/removed without updating this census, or a sentence was reworded/reflowed so \
             this exact needle no longer matches (markup inside the phrase counts as rewording — \
             re-anchor the needle or restore the phrase)"
        );
    };
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let facade = std::fs::read_to_string(format!("{root}/docs/modules/facade.md"))
        .expect("docs/modules/facade.md is readable from the workspace");
    let site = std::fs::read_to_string(format!("{root}/site/reference.html"))
        .expect("site/reference.html is readable from the workspace");

    let window_dash = format!("{MIN_FILES_PER_COMMIT}\u{2013}{MAX_FILES_PER_COMMIT} files");
    let window_range = format!("{MIN_FILES_PER_COMMIT}..={MAX_FILES_PER_COMMIT} files");
    let top_partners = format!("top {COUPLING_TOP_PER_FILE} co-change partners");
    let top_short = format!("top {COUPLING_TOP_PER_FILE} partners");
    count(&facade, &window_dash, 2, "docs/modules/facade.md");
    count(&facade, &window_range, 1, "docs/modules/facade.md");
    count(&facade, &shipped, 1, "docs/modules/facade.md");
    count(&facade, &top_partners, 1, "docs/modules/facade.md");
    count(&facade, &top_short, 1, "docs/modules/facade.md");
    let window_entity = format!("{MIN_FILES_PER_COMMIT}&ndash;{MAX_FILES_PER_COMMIT} files");
    count(&site, &window_entity, 1, "site/reference.html");
    count(&site, &shipped, 1, "site/reference.html");
    count(&site, &top_partners, 1, "site/reference.html");

    // The layerCoChurn row's OWN two thresholds ride the same doc sentence and were the
    // half-de-numbered stragglers the 2026-08-15 review caught: pin them to their owning constants
    // too (`zzop_metrics::cross_layer_co_churn`), same census discipline.
    use zzop_metrics::{MIN_CO_CHANGES, TOP_PAIRS};
    let churn_floor = format!("below {MIN_CO_CHANGES} co-changes");
    let churn_rows = format!("top {TOP_PAIRS} rows");
    count(&facade, &churn_floor, 1, "docs/modules/facade.md");
    count(&facade, &churn_rows, 1, "docs/modules/facade.md");
}
