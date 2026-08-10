use super::*;

fn node(source: &str, rel: &str) -> DepNode {
    DepNode {
        source: source.to_string(),
        rel: rel.to_string(),
        loc: Some(10),
        git: None,
    }
}

/// Builds the `(ids, nodes)` pair `collapse` wants, for a single-tree run where id == rel.
fn universe(rels: &[&str]) -> (Vec<String>, Vec<DepNode>) {
    (
        rels.iter().map(|r| r.to_string()).collect(),
        rels.iter().map(|r| node("t", r)).collect(),
    )
}

fn in_scope<'a>(ids: &'a [String], nodes: &'a [DepNode]) -> BTreeMap<&'a String, &'a DepNode> {
    ids.iter().zip(nodes.iter()).collect()
}

fn edges(pairs: &[(&str, &str)]) -> BTreeSet<(String, String)> {
    pairs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[test]
fn many_file_edges_between_two_boxes_become_one_edge_carrying_their_count() {
    let (ids, nodes) = universe(&[
        "crates/engine/a.rs",
        "crates/engine/b.rs",
        "crates/core/x.rs",
        "crates/core/y.rs",
    ]);
    let f = collapse(
        Fold::of(Some(2)),
        &in_scope(&ids, &nodes),
        &edges(&[
            ("crates/engine/a.rs", "crates/core/x.rs"),
            ("crates/engine/a.rs", "crates/core/y.rs"),
            ("crates/engine/b.rs", "crates/core/x.rs"),
        ]),
        &BTreeSet::new(),
    );
    assert_eq!(f.nodes.len(), 2);
    assert_eq!(f.edges.len(), 1);
    assert_eq!(
        f.edges
            .get(&("crates/engine".to_string(), "crates/core".to_string())),
        Some(&3),
        "the label must be the number of FILE edges collapsed, not 1 and not a strength"
    );
    assert_eq!(f.file_edges, 3);
}

#[test]
fn an_edge_inside_one_box_is_dropped_rather_than_drawn_as_a_self_loop() {
    let (ids, nodes) = universe(&["crates/engine/a.rs", "crates/engine/b.rs"]);
    let f = collapse(
        Fold::of(Some(2)),
        &in_scope(&ids, &nodes),
        &edges(&[("crates/engine/a.rs", "crates/engine/b.rs")]),
        &BTreeSet::new(),
    );
    assert_eq!(f.nodes.len(), 1);
    assert!(
        f.edges.is_empty(),
        "a module importing itself is what a module IS"
    );
    // Still counted as a file edge that participated: the census must not make the collapse look free.
    assert_eq!(f.file_edges, 1);
}

#[test]
fn an_edge_whose_far_end_was_scoped_out_does_not_reach_the_picture() {
    let (ids, nodes) = universe(&["crates/engine/a.rs"]);
    let f = collapse(
        Fold::of(Some(2)),
        &in_scope(&ids, &nodes),
        // `crates/core/x.rs` is not in scope, so this edge is not evidence about a visible box.
        &edges(&[("crates/engine/a.rs", "crates/core/x.rs")]),
        &BTreeSet::new(),
    );
    assert!(f.edges.is_empty());
    assert_eq!(f.file_edges, 0);
}

#[test]
fn a_box_is_marked_in_cycle_when_any_file_inside_it_is() {
    let (ids, nodes) = universe(&["crates/engine/a.rs", "crates/engine/b.rs"]);
    let mut cycle = BTreeSet::new();
    cycle.insert("crates/engine/b.rs".to_string());
    let f = collapse(
        Fold::of(Some(2)),
        &in_scope(&ids, &nodes),
        &edges(&[]),
        &cycle,
    );
    assert!(f.in_cycle.contains("crates/engine"));
}

#[test]
fn a_box_holding_a_deeper_path_is_not_counted_as_unfoldable() {
    // `README.md` cannot fold to 2 segments; `crates/engine` can. Only the first is the caveat's subject.
    let (ids, nodes) = universe(&["README.md", "crates/engine/a.rs"]);
    let f = collapse(
        Fold::of(Some(2)),
        &in_scope(&ids, &nodes),
        &edges(&[]),
        &BTreeSet::new(),
    );
    assert_eq!(f.nodes.len(), 2);
    assert_eq!(f.unfoldable, 1);
}

#[test]
fn a_box_mixing_a_short_path_and_a_deep_one_is_a_real_group_not_a_lone_file() {
    // Both fold to `src`: one is a root-ish two-segment file, one is deeper. The box stands for more
    // than a single file, so the "SINGLE FILE" caveat would be false about it.
    let (ids, nodes) = universe(&["src/main.rs", "src/deep/inner.rs"]);
    let f = collapse(
        Fold::of(Some(1)),
        &in_scope(&ids, &nodes),
        &edges(&[]),
        &BTreeSet::new(),
    );
    assert_eq!(f.nodes.len(), 1);
    assert_eq!(f.unfoldable, 0);
}

#[test]
fn two_trees_keep_separate_boxes_for_the_same_folded_path() {
    let ids = vec!["fe::src/a.ts".to_string(), "be::src/b.ts".to_string()];
    let nodes = vec![node("fe", "src/a.ts"), node("be", "src/b.ts")];
    let f = collapse(
        Fold::of(Some(1)),
        &in_scope(&ids, &nodes),
        &edges(&[]),
        &BTreeSet::new(),
    );
    assert_eq!(
        f.nodes.len(),
        2,
        "two trees' `src/` are two different boxes"
    );
    assert!(f.nodes.contains_key("fe::src"));
    assert!(f.nodes.contains_key("be::src"));
}

#[test]
fn a_folded_box_carries_no_measured_axes() {
    let (ids, nodes) = universe(&["crates/engine/a.rs", "crates/engine/b.rs"]);
    let f = collapse(
        Fold::of(Some(2)),
        &in_scope(&ids, &nodes),
        &edges(&[]),
        &BTreeSet::new(),
    );
    let b = &f.nodes["crates/engine"];
    assert_eq!(
        b.loc, None,
        "a box has no single loc; summing some axes and not others is worse"
    );
    assert!(b.git.is_none());
}
