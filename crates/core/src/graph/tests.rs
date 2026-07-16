//! Exercises `connected_components`: isolated nodes, fully-connected islands, multiple islands sorted
//! descending by size, direction-agnostic edges, self-loops, edges with an out-of-set endpoint, and
//! alphabetical tie-breaking by first node.
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

#[test]
fn no_edges_each_node_own_component() {
    let r = connected_components(&ids(&["a", "b", "c"]), &[]);
    assert_eq!(r.sizes, vec![1, 1, 1]);
    let distinct: std::collections::HashSet<_> = r.component_by_node.values().collect();
    assert_eq!(distinct.len(), 3);
}

#[test]
fn fully_connected_single_island() {
    let r = connected_components(&ids(&["a", "b", "c"]), &[edge("a", "b"), edge("b", "c")]);
    assert_eq!(r.sizes, vec![3]);
    let distinct: std::collections::HashSet<_> = r.component_by_node.values().copied().collect();
    assert_eq!(distinct, std::collections::HashSet::from([0]));
}

#[test]
fn two_islands_sorted_desc() {
    let r = connected_components(
        &ids(&["a", "b", "c", "d"]),
        &[edge("a", "b"), edge("b", "c")],
    );
    assert_eq!(r.sizes, vec![3, 1]);
    assert_eq!(r.component_by_node["a"], 0);
    assert_eq!(r.component_by_node["b"], 0);
    assert_eq!(r.component_by_node["c"], 0);
    assert_eq!(r.component_by_node["d"], 1);
}

#[test]
fn direction_ignored() {
    let r1 = connected_components(&ids(&["a", "b"]), &[edge("a", "b")]);
    let r2 = connected_components(&ids(&["a", "b"]), &[edge("b", "a")]);
    assert_eq!(r1.sizes, r2.sizes);
}

#[test]
fn self_loop_no_effect() {
    let r = connected_components(&ids(&["a"]), &[edge("a", "a")]);
    assert_eq!(r.sizes, vec![1]);
}

#[test]
fn ghost_endpoints_ignored() {
    let r = connected_components(&ids(&["a", "b"]), &[edge("a", "b"), edge("a", "ghost")]);
    assert_eq!(r.sizes, vec![2]);
}

#[test]
fn complex_three_islands() {
    let r = connected_components(
        &ids(&["a", "b", "c", "d", "e", "f", "g"]),
        &[
            edge("a", "b"),
            edge("b", "c"),
            edge("c", "d"),
            edge("e", "f"),
        ],
    );
    assert_eq!(r.sizes, vec![4, 2, 1]);
}

#[test]
fn tie_break_by_alphabetical_first_node() {
    let r = connected_components(
        &ids(&["z", "y", "a", "b"]),
        &[edge("a", "b"), edge("z", "y")],
    );
    assert_eq!(r.sizes, vec![2, 2]);
    assert_eq!(r.component_by_node["a"], 0); // first node "a" < "z"
    assert_eq!(r.component_by_node["z"], 1);
}

// --- find_cycles (directed edges) ---

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
