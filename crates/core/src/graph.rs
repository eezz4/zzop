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
mod tests;
