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

/// A SELF-EDGE AT THE FILE-LEVEL BOUNDARY IS NOT A CYCLE, while `find_cycles` must keep reporting one
/// (`cycles_self_loop` above pins that, and a self-loop really is a cycle in a digraph). The split is
/// the point: what is false is the EDGE, not the graph theory.
///
/// Two outside audits produced the same artifact the same week, in two languages that cannot express
/// the dependency the edge claims. C# (aspnetcore `cc827b0a0b`, and the audit's only HARMFUL verdict):
/// `using <the namespace this file declares>`, whose only available "fix" is deleting the using — CS0246,
/// because an assembly-level attribute ahead of the file-scoped namespace resolves through it. Go
/// (terraform `799c8d8963`): a `//go:build ignore` `package main` importing its own directory, where
/// excluding that single directory moved `architecture.pain` 94.1 -> 61.8 and the `defect` axis — import
/// cycles only — 32.3 -> 0.0, i.e. the whole axis was this one artifact.
#[test]
fn a_file_that_resolves_an_import_onto_itself_is_not_a_cycle() {
    let mut dep: crate::ir::DepGraph = Default::default();
    dep.insert(
        "oauthsrv/main.go".to_string(),
        vec!["oauthsrv/main.go".to_string()],
    );
    assert!(
        circular_from_dep(&dep).is_empty(),
        "a one-member cycle leaves `circular`'s remedies undefined — there is no second side to \
         invert: {:?}",
        circular_from_dep(&dep)
    );
    // And the filter must not reach a REAL two-file cycle, which is the whole population this rule is for.
    let mut real: crate::ir::DepGraph = Default::default();
    real.insert("a.ts".to_string(), vec!["b.ts".to_string()]);
    real.insert("b.ts".to_string(), vec!["a.ts".to_string()]);
    // Member order inside an SCC is Tarjan's discovery order, which is why this file has `sorted`.
    let real_cycles = circular_from_dep(&real);
    assert_eq!(real_cycles.len(), 1, "{real_cycles:?}");
    assert_eq!(
        sorted(real_cycles[0].clone()),
        ids(&["a.ts", "b.ts"]),
        "the two-member cycle is the subject this rule exists for and must survive the self-edge filter"
    );
    // A self-edge SITTING BESIDE a real cycle must drop the edge and keep the cycle.
    let mut both: crate::ir::DepGraph = Default::default();
    both.insert(
        "a.ts".to_string(),
        vec!["b.ts".to_string(), "a.ts".to_string()],
    );
    both.insert("b.ts".to_string(), vec!["a.ts".to_string()]);
    let both_cycles = circular_from_dep(&both);
    assert_eq!(both_cycles.len(), 1, "{both_cycles:?}");
    assert_eq!(
        sorted(both_cycles[0].clone()),
        ids(&["a.ts", "b.ts"]),
        "dropping the self-edge must not drop the cycle the same file is genuinely in"
    );
}
