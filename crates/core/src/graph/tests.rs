//! Exercises `find_cycles`: no-cycle graphs, two-/three-node cycles, self-loops, multiple cycles
//! sorted largest first, nested cycles merging into one SCC, and deep chains (iterative Tarjan must
//! not overflow the stack).
use super::*;

fn ids(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}
fn edge(s: &str, t: &str) -> ComponentEdge {
    ComponentEdge {
        source: s.into(),
        target: t.into(),
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

#[test]
fn cycles_none_is_empty() {
    assert!(find_cycles(&ids(&["a#x", "a#y"]), &[edge("a#x", "a#y")]).is_empty());
}

#[test]
fn cycles_two_node() {
    let c = find_cycles(
        &ids(&["a#x", "a#y"]),
        &[edge("a#x", "a#y"), edge("a#y", "a#x")],
    );
    assert_eq!(c.len(), 1);
    assert_eq!(sorted(c[0].clone()), ids(&["a#x", "a#y"]));
}

#[test]
fn cycles_three_node() {
    let c = find_cycles(
        &ids(&["a", "b", "c"]),
        &[edge("a", "b"), edge("b", "c"), edge("c", "a")],
    );
    assert_eq!(c.len(), 1);
    assert_eq!(sorted(c[0].clone()), ids(&["a", "b", "c"]));
}

#[test]
fn cycles_self_loop() {
    let c = find_cycles(&ids(&["a"]), &[edge("a", "a")]);
    assert_eq!(c, vec![ids(&["a"])]);
}

#[test]
fn cycles_size1_no_self_ref_not_a_cycle() {
    assert!(find_cycles(&ids(&["a", "b"]), &[edge("a", "b")]).is_empty());
}

#[test]
fn cycles_multiple_sorted_largest_first() {
    let c = find_cycles(
        &ids(&["a", "b", "c", "d", "e"]),
        &[
            edge("a", "b"),
            edge("b", "a"),
            edge("c", "d"),
            edge("d", "e"),
            edge("e", "c"),
        ],
    );
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].len(), 3);
    assert_eq!(c[1].len(), 2);
}

#[test]
fn cycles_nested_merge_into_one() {
    let c = find_cycles(
        &ids(&["a", "b", "c"]),
        &[
            edge("a", "b"),
            edge("b", "a"),
            edge("b", "c"),
            edge("c", "b"),
        ],
    );
    assert_eq!(c.len(), 1);
    assert_eq!(sorted(c[0].clone()), ids(&["a", "b", "c"]));
}

#[test]
fn cycles_deep_linear_chain_no_overflow() {
    // backlog #92 — iterative Tarjan must not overflow on deep chains.
    let n = 100_000;
    let nodes: Vec<String> = (0..n).map(|i| format!("n{i}")).collect();
    let edges: Vec<ComponentEdge> = (0..n - 1)
        .map(|i| edge(&format!("n{i}"), &format!("n{}", i + 1)))
        .collect();
    assert!(find_cycles(&nodes, &edges).is_empty());
}

#[test]
fn cycles_deep_cyclic_chain_single_scc() {
    let n = 50_000usize;
    let nodes: Vec<String> = (0..n).map(|i| format!("n{i}")).collect();
    let edges: Vec<ComponentEdge> = (0..n)
        .map(|i| edge(&format!("n{i}"), &format!("n{}", (i + 1) % n)))
        .collect();
    let c = find_cycles(&nodes, &edges);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].len(), n);
}
