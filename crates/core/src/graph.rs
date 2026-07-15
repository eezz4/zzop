//! Graph primitives — shared by the engine and native rules (rules/native/*).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentEdge {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Default)]
pub struct ConnectedComponentsResult {
    /// nodeId -> componentId (0-based, largest component = 0).
    pub component_by_node: HashMap<String, usize>,
    /// node count per componentId (index = componentId, sorted desc by size).
    pub sizes: Vec<usize>,
}

/// Connected components of an undirected graph (Union-Find). Direction ignored; isolated nodes are size-1.
/// Edges whose source/target is not in nodeIds are ignored. Ties break on the group's first node (input order), lexicographically.
pub fn connected_components(
    node_ids: &[String],
    edges: &[ComponentEdge],
) -> ConnectedComponentsResult {
    let mut parent: HashMap<String, String> =
        node_ids.iter().map(|id| (id.clone(), id.clone())).collect();

    for e in edges {
        if parent.contains_key(&e.source) && parent.contains_key(&e.target) {
            union(&mut parent, &e.source, &e.target);
        }
    }

    // Group nodes by root — preserve nodeIds input order (basis for the tie-break first node).
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for id in node_ids {
        let r = find(&mut parent, id);
        groups.entry(r).or_default().push(id.clone());
    }

    // Sort by size desc, then first node lexicographically. (group[0] is unique per group -> total order.)
    let mut sorted: Vec<Vec<String>> = groups.into_values().collect();
    sorted.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));

    let mut component_by_node = HashMap::new();
    let mut sizes = Vec::with_capacity(sorted.len());
    for (idx, group) in sorted.iter().enumerate() {
        sizes.push(group.len());
        for id in group {
            component_by_node.insert(id.clone(), idx);
        }
    }

    ConnectedComponentsResult {
        component_by_node,
        sizes,
    }
}

fn find(parent: &mut HashMap<String, String>, x: &str) -> String {
    // Find the root.
    let mut cur = x.to_string();
    while parent[&cur] != cur {
        cur = parent[&cur].clone();
    }
    // Path compression.
    let mut walker = x.to_string();
    while parent[&walker] != cur {
        let next = parent[&walker].clone();
        parent.insert(walker.clone(), cur.clone());
        walker = next;
    }
    cur
}

fn union(parent: &mut HashMap<String, String>, a: &str, b: &str) {
    let ra = find(parent, a);
    let rb = find(parent, b);
    if ra != rb {
        parent.insert(ra, rb);
    }
}

/// Strongly connected components that are cycles — SCCs of size >= 2, or a size-1 SCC with a self-loop.
/// Sorted largest first, then by first node. Edges are directed (source -> target). Uses an iterative
/// Tarjan's algorithm — no recursion, so deep graphs don't overflow the stack.
pub fn find_cycles(node_ids: &[String], edges: &[ComponentEdge]) -> Vec<Vec<String>> {
    let adj = build_adjacency(edges);
    let mut sccs = tarjan(node_ids, &adj);
    sccs.retain(|scc| {
        scc.len() >= 2
            || (scc.len() == 1 && adj.get(&scc[0]).is_some_and(|ns| ns.contains(&scc[0])))
    });
    sccs.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    sccs
}

/// File-level circular dependencies from a dep graph: nodes = all files, edges = dep. Thin wrapper over
/// [`circular_from_dep_excluding`] with an empty exclusion set — every `DepGraph` edge is a candidate
/// cycle edge. Node/edge order is made deterministic (sorted) since a DepGraph (HashMap) has no stable
/// iteration order.
pub fn circular_from_dep(dep: &crate::ir::DepGraph) -> Vec<Vec<String>> {
    circular_from_dep_excluding(dep, &std::collections::HashSet::new())
}

/// `circular_from_dep`, additionally skipping any `(from, to)` edge present in `excluded` — the ephemeral
/// type-only-edge set a caller computes alongside its own dep-graph build (e.g.
/// `zzop_parser_typescript::lang::resolve::build_dep`/`build_dep_with_workspace`'s second return value):
/// a file pair linked ONLY by `import type`/per-specifier `{ type X }` bindings must not read as a
/// circular dependency, since no runtime module-load edge exists between them. `excluded` is deliberately
/// never cached/serialized by any caller — circular detection is a non-cached whole-graph pass, so this
/// set only needs to live for the duration of one analysis run.
pub fn circular_from_dep_excluding(
    dep: &crate::ir::DepGraph,
    excluded: &std::collections::HashSet<(String, String)>,
) -> Vec<Vec<String>> {
    let mut node_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut edges = Vec::new();
    for (from, tos) in dep {
        node_set.insert(from.clone());
        for to in tos {
            node_set.insert(to.clone());
            if excluded.contains(&(from.clone(), to.clone())) {
                continue;
            }
            edges.push(ComponentEdge {
                source: from.clone(),
                target: to.clone(),
            });
        }
    }
    edges.sort_by(|a, b| a.source.cmp(&b.source).then(a.target.cmp(&b.target)));
    let nodes: Vec<String> = node_set.into_iter().collect();
    find_cycles(&nodes, &edges)
}

fn build_adjacency(edges: &[ComponentEdge]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for e in edges {
        map.entry(e.source.clone())
            .or_default()
            .push(e.target.clone());
    }
    map
}

/// Iterative Tarjan SCC with an explicit work stack — mirrors the recursive post-order lowlink update.
/// Preserves node/edge input order, producing deterministic SCCs and discovery order.
fn tarjan(nodes: &[String], adj: &HashMap<String, Vec<String>>) -> Vec<Vec<String>> {
    let mut index = 0i64;
    let mut stack: Vec<String> = Vec::new();
    let mut on_stack: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut index_map: HashMap<String, i64> = HashMap::new();
    let mut lowlink: HashMap<String, i64> = HashMap::new();
    let mut sccs: Vec<Vec<String>> = Vec::new();

    for start in nodes {
        if index_map.contains_key(start) {
            continue;
        }
        let mut work: Vec<(String, usize)> = vec![(start.clone(), 0)];
        while !work.is_empty() {
            let li = work.len() - 1;
            let v = work[li].0.clone();
            let iv = work[li].1;
            if iv == 0 {
                index_map.insert(v.clone(), index);
                lowlink.insert(v.clone(), index);
                index += 1;
                stack.push(v.clone());
                on_stack.insert(v.clone());
            }
            let neighbors = adj.get(&v);
            let nlen = neighbors.map_or(0, |n| n.len());
            if iv < nlen {
                let w = neighbors.unwrap()[iv].clone();
                work[li].1 += 1;
                if !index_map.contains_key(&w) {
                    work.push((w, 0));
                } else if on_stack.contains(&w) {
                    let nv = lowlink[&v].min(index_map[&w]);
                    lowlink.insert(v.clone(), nv);
                }
                continue;
            }
            if lowlink[&v] == index_map[&v] {
                sccs.push(pop_scc(&mut stack, &mut on_stack, &v));
            }
            work.pop();
            if let Some(last) = work.last() {
                let parent = last.0.clone();
                let nv = lowlink[&parent].min(lowlink[&v]);
                lowlink.insert(parent, nv);
            }
        }
    }
    sccs
}

fn pop_scc(
    stack: &mut Vec<String>,
    on_stack: &mut std::collections::HashSet<String>,
    root: &str,
) -> Vec<String> {
    let mut scc = Vec::new();
    while let Some(w) = stack.pop() {
        on_stack.remove(&w);
        let is_root = w == root;
        scc.push(w);
        if is_root {
            break;
        }
    }
    scc
}

#[cfg(test)]
mod tests {
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
        let distinct: std::collections::HashSet<_> =
            r.component_by_node.values().copied().collect();
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
}
