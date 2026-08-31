//! Traversal half of the call-graph substrate: downstream-only reachability over a [`SymbolGraph`].
//! Takes the graph as given — how those edges were resolved (and what was dropped building them) is
//! [`super::resolve`]'s business.

use std::collections::{BTreeMap, HashMap};

use super::SymbolGraph;

/// Downstream-only BFS depth map from `start` over `graph` (the only direction the two call-graph rules
/// need; nodeId -> depth, 0 = `start`). Unreachable nodes are simply absent from the map.
///
/// Unexported (`pub(super)` reaches the `callgraph` module's own tests and nothing else):
/// `bfs_reachable` is the only production caller and the only shape any consumer has ever wanted
/// ("closest reached site wins"). Publishing the raw depth map added a second entry point nobody
/// used — widen it again when a caller outside this module actually needs the whole map.
pub(super) fn bfs_depths(graph: &SymbolGraph, start: &str) -> BTreeMap<String, u32> {
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in graph {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }

    let mut depth_by_node: BTreeMap<String, u32> = BTreeMap::new();
    depth_by_node.insert(start.to_string(), 0);
    let mut frontier: Vec<String> = vec![start.to_string()];
    let mut depth = 0u32;
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for node in &frontier {
            let Some(neighbors) = adjacency.get(node.as_str()) else {
                continue;
            };
            for &neighbor in neighbors {
                if !depth_by_node.contains_key(neighbor) {
                    depth_by_node.insert(neighbor.to_string(), depth + 1);
                    next.push(neighbor.to_string());
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
        depth += 1;
    }
    depth_by_node
}

/// The reachable node (downstream of `start`, `bfs_depths` semantics) with the lowest depth for which
/// `predicate` holds, tie-broken by symbol id ascending for determinism (this crate's general convention —
/// see `registry::merge_findings`'s explicit tie-breaks — since `bfs_depths`' `BTreeMap` iteration order is
/// already id-sorted, not BFS-discovery order). Returns `None` when no reachable node (including `start`
/// itself) satisfies `predicate`. This is the shared "closest reached site wins" primitive behind
/// `scanUnsafeReadEndpoint` / `scanNonIdempotentWrite`.
pub fn bfs_reachable(
    graph: &SymbolGraph,
    start: &str,
    predicate: impl Fn(&str) -> bool,
) -> Option<(String, u32)> {
    bfs_depths(graph, start)
        .into_iter()
        .filter(|(id, _)| predicate(id))
        .min_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
}
