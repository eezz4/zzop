//! Unreachable-code detector — closed "dead islands": files imported in-repo (fanIn > 0) yet not reachable from any
//! entrypoint. Pure over (nodes, dep), language-agnostic.
//!
//! `unreachable_findings` is the `"unreachable"` native-analysis Finding-shaping wrapper the engine calls
//! (moved here alongside the algorithm it shapes).
//!
//! **vs. [`crate::dead_candidates`]**: disjoint by construction. That module only ever flags a file with
//! ZERO importers (`fan_in == 0`); this one only ever flags a file with ONE OR MORE importers (`fan_in >
//! 0`, see `find_unreachable`'s own `n.fan_in > 0` filter below) that are themselves unreachable from any
//! entrypoint — a closed island, not an orphan. A given file can never be flagged by both.

use std::collections::{HashSet, VecDeque};

use zzop_core::{disable_hint, DepGraph, FileNode, Finding, Severity};

mod landing;
mod patterns;

use landing::ISLAND_DELETION_LANDING;
use patterns::entry_patterns;
pub use patterns::is_tool_config_file;
pub(crate) use patterns::{framework_route_patterns, is_tool_entry_file};
use zzop_core::is_test_file;

#[derive(Debug, Clone, PartialEq)]
pub struct UnreachableFile {
    pub path: String,
    pub loc: u32,
    pub risk_score: f64,
    pub fan_in: u32,
}

/// Files with fanIn > 0 that no entrypoint reaches — closed dead islands. Ranked by loc desc, then risk, then path.
/// Entrypoints = conventional entry files + test files + every fanIn=0 file (false-positive-safe) +
/// `extra_entries` — paths the CALLER knows are loaded by a mechanism this graph can't see (the same
/// contract as `find_dead_candidates`' parameter of the same name): cargo-manifest-declared target
/// files (`[[bin]]`/`[[test]]`/... `path = "..."` — loaded by cargo, never imported) and Mode-B
/// adapter-overlay files marked `is_entry`.
pub fn find_unreachable(
    nodes: &[FileNode],
    dep: &DepGraph,
    limit: usize,
    extra_entries: &HashSet<String>,
) -> Vec<UnreachableFile> {
    let mut entries: HashSet<String> = HashSet::new();
    for n in nodes {
        if n.fan_in == 0
            || is_entry_file(&n.path)
            || is_test_file(&n.path)
            || extra_entries.contains(&n.path)
        {
            entries.insert(n.path.clone());
        }
    }
    let reachable = forward_closure(&entries, dep);

    let mut out: Vec<UnreachableFile> = nodes
        .iter()
        .filter(|n| n.fan_in > 0 && !reachable.contains(&n.path) && !is_test_file(&n.path))
        .map(|n| UnreachableFile {
            path: n.path.clone(),
            loc: n.loc,
            risk_score: n.risk_score,
            fan_in: n.fan_in,
        })
        .collect();
    out.sort_by(|a, b| {
        b.loc
            .cmp(&a.loc)
            .then(
                b.risk_score
                    .partial_cmp(&a.risk_score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.path.cmp(&b.path))
    });
    out.truncate(limit);
    out
}

/// One `Finding` per unreachable file (native analysis id `"unreachable"`, matching
/// `register_native_analyses`). No `limit` param — the engine's own call site passes `nodes.len()`.
pub fn unreachable_findings(
    nodes: &[FileNode],
    dep: &DepGraph,
    extra_entries: &HashSet<String>,
) -> Vec<Finding> {
    find_unreachable(nodes, dep, nodes.len(), extra_entries)
        .into_iter()
        .map(|u| Finding {
            rule_id: "unreachable".to_string(),
            severity: Severity::Info,
            file: u.path,
            line: 1,
            message: format!(
                "file has {} importer(s) in this tree but is unreachable from any entrypoint — its \
                 importers form a closed island nothing outside it can reach, so it's effectively dead \
                 despite having in-repo references. Whether that is true rests entirely on the entrypoint \
                 set, so read it before you treat this as dead. {ISLAND_DELETION_LANDING} IF NO SUCH \
                 LOADER EXISTS: delete the island, or wire it back to a real entrypoint if it should be \
                 reachable. {} if this island is reached by a mechanism this \
                 graph doesn't see (e.g. dynamic `require`, a plugin loader).",
                u.fan_in,
                disable_hint("unreachable")
            ),
            evidence_paths: Vec::new(),
            data: Some(serde_json::json!({ "loc": u.loc, "fan_in": u.fan_in })),
        })
        .collect()
}

/// All files reachable by following import edges forward from the entry set (BFS).
fn forward_closure(entries: &HashSet<String>, dep: &DepGraph) -> HashSet<String> {
    let mut seen: HashSet<String> = entries.clone();
    let mut queue: VecDeque<String> = entries.iter().cloned().collect();
    while let Some(cur) = queue.pop_front() {
        if let Some(nexts) = dep.get(&cur) {
            for next in nexts {
                if seen.insert(next.clone()) {
                    queue.push_back(next.clone());
                }
            }
        }
    }
    seen
}

fn is_entry_file(path: &str) -> bool {
    entry_patterns().iter().any(|re| re.is_match(path))
}

#[cfg(test)]
mod tests;
