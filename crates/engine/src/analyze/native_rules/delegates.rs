//! Thin delegates to `zzop_rules_graph` plus the local `DepStats` builder — kept as `crate::analyze`
//! functions (rather than inlining the calls at every call site) since `envelope::analyze_envelope`
//! also imports them by this name/path.

use std::collections::HashSet;

use zzop_core::{DepGraph, DepStats, FileNode, Finding};

/// Fan-in/fan-out/all-paths derived from a resolved dep graph — the minimal `DepStats`-shaped input
/// `build_file_nodes` needs. A local build since `zzop_core::file_nodes` has no standalone "DepStats
/// from a DepGraph" helper.
pub(crate) fn dep_stats_from_dep(dep: &DepGraph) -> DepStats {
    let mut fan_in = std::collections::BTreeMap::new();
    let mut fan_out = std::collections::BTreeMap::new();
    let mut all_paths = std::collections::BTreeSet::new();
    for (src, targets) in dep {
        all_paths.insert(src.clone());
        fan_out.insert(src.clone(), targets.len() as u32);
        for target in targets {
            all_paths.insert(target.clone());
            *fan_in.entry(target.clone()).or_insert(0) += 1;
        }
    }
    DepStats {
        fan_in,
        fan_out,
        all_paths,
    }
}

/// Thin delegate to `zzop_rules_graph::circular_findings`. Kept as a `crate::analyze` function (rather
/// than inlining the call at every call site) since `envelope::analyze_envelope` also imports it by
/// this name/path. `cycles` is passed in (rather than re-derived from `dep`) so this and the
/// scores/recommendations computations above share one `circular_from_dep` call.
pub(crate) fn circular_findings(cycles: &[Vec<String>]) -> Vec<Finding> {
    zzop_rules_graph::circular_findings(cycles)
}

/// Thin delegate to `zzop_rules_graph::unreachable_findings` — see `circular_findings`'s doc for why this
/// wrapper stays here rather than being inlined at its call sites. `extra_entries` forwards straight
/// through (package.json/cargo-declared entry files that would otherwise false-positive as unreachable).
pub(crate) fn unreachable_findings(
    nodes: &[FileNode],
    dep: &DepGraph,
    extra_entries: &HashSet<String>,
) -> Vec<Finding> {
    zzop_rules_graph::unreachable_findings(nodes, dep, extra_entries)
}

/// Thin delegate to `zzop_rules_graph::dead_candidate_findings` — see `circular_findings`'s doc. `extra_entries`
/// forwards straight through (package.json-referenced entry files).
pub(crate) fn dead_candidate_findings(
    nodes: &[FileNode],
    dep: &DepGraph,
    extra_entries: &HashSet<String>,
) -> Vec<Finding> {
    zzop_rules_graph::dead_candidate_findings(nodes, dep, extra_entries)
}
