//! Traversal half of the call-graph substrate: downstream-only reachability over a [`SymbolGraph`].
//! Takes the graph as given — how those edges were resolved (and what was dropped building them) is
//! [`super::resolve`]'s business.

use std::collections::HashMap;

use super::SymbolGraph;

/// The graph's edge list turned into an adjacency index, built ONCE and walked many times.
///
/// ## Why this type exists
/// `bfs_depths` used to build this map inside itself, on every call. That reads as a detail and is
/// not one: the rules that walk this graph call it ONCE PER ROUTE, so the cost was
/// O(routes x edges) — quadratic in the size of the tree, for a traversal that is linear.
///
/// 📏 Measured 2026-09-08 on a synthetic 3,000-file TypeScript tree with 3,000 routes (review ledger
/// V112): `unsafe-read-endpoint` 10.62s and `non-idempotent-write` 10.22s, out of a 22.9s run —
/// **91% of the whole analysis, for two rules that reported zero findings**. Both spent it here,
/// rebuilding the same map 3,000 times each.
///
/// Borrowed, never owned: the index points into the `SymbolGraph` it was built from, so building it
/// copies no ids and holds the graph still for its lifetime.
pub struct Adjacency<'a> {
    by_from: HashMap<&'a str, Vec<&'a str>>,
}

impl<'a> Adjacency<'a> {
    /// One pass over the edge list. Call this once per graph, outside whatever loop walks it.
    /// One pass over the edge list. Call this once per graph, outside whatever loop walks it.
    pub fn build(graph: &'a SymbolGraph) -> Self {
        let mut by_from: HashMap<&'a str, Vec<&'a str>> = HashMap::new();
        for edge in graph {
            by_from
                .entry(edge.from.as_str())
                .or_default()
                .push(edge.to.as_str());
        }
        Self { by_from }
    }
}

/// Downstream-only BFS depth map from `start` over an already-built [`Adjacency`] (the only
/// direction the two call-graph rules need; nodeId -> depth, 0 = `start`). Unreachable nodes are
/// simply absent from the map.
///
/// Keys BORROW the graph, and the map is a `HashMap` rather than a `BTreeMap`. Both were `String` and
/// sorted, which meant one heap allocation per visited node and an ordered insert per node, paid
/// again for every route. Neither buys anything: the only consumer is `bfs_reachable_in`, whose
/// `min_by` already breaks depth ties by id — so the map's iteration order cannot reach a verdict,
/// and dropping the ordering leaves the answer identical (review ledger V112).
///
/// Unexported (`pub(super)` reaches the `callgraph` module's own tests and nothing else):
/// `bfs_reachable_in` is the only caller and the only shape any consumer has ever wanted
/// ("closest reached site wins"). Publishing the raw depth map added a second entry point nobody
/// used — widen it again when a caller outside this module actually needs the whole map.
pub(super) fn bfs_depths<'a>(adjacency: &Adjacency<'a>, start: &'a str) -> HashMap<&'a str, u32> {
    let mut depth_by_node: HashMap<&'a str, u32> = HashMap::new();
    depth_by_node.insert(start, 0);
    let mut frontier: Vec<&'a str> = vec![start];
    let mut depth = 0u32;
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for node in &frontier {
            let Some(neighbors) = adjacency.by_from.get(node) else {
                continue;
            };
            for &neighbor in neighbors {
                if !depth_by_node.contains_key(neighbor) {
                    depth_by_node.insert(neighbor, depth + 1);
                    next.push(neighbor);
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
/// see `registry::merge_findings`'s explicit tie-breaks. The tie-break is LOAD-BEARING rather than
/// decorative: `bfs_depths` returns an unordered map). Returns `None` when no reachable node (including `start`
/// itself) satisfies `predicate`. This is the shared "closest reached site wins" primitive behind
/// `scanUnsafeReadEndpoint` / `scanNonIdempotentWrite`.
///
/// Takes the [`Adjacency`] rather than the graph, and that is the whole API: there used to be a
/// `bfs_reachable(graph, …)` wrapper beside it that built the index inline, kept "for one-shot callers
/// and for tests". 📏 Recounted 2026-09-08: production callers **zero** — the only callers were four of
/// this module's own tests, which is a wrapper existing to be tested (review ledger V120). It went. A
/// caller with one graph and one query writes `Adjacency::build(&g)` at the call site, which is the
/// line the wrapper was hiding — and hiding it is what let a rule build the index once per route
/// without anyone noticing (V112).
pub fn bfs_reachable_in<'a>(
    adjacency: &Adjacency<'a>,
    start: &'a str,
    predicate: impl Fn(&str) -> bool,
) -> Option<(String, u32)> {
    bfs_depths(adjacency, start)
        .into_iter()
        .filter(|(id, _)| predicate(id))
        .min_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)))
        .map(|(id, depth)| (id.to_string(), depth))
}
