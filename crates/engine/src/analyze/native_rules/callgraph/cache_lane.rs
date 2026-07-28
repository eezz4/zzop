//! The `cache-lane-file-read` arm of `run_callgraph_rules` — the first consumer of this pass that has
//! nothing to do with HTTP.
//!
//! Split out of `mod.rs` for the line cap, but the seam is a real one: everything else in that file
//! reasons about routes, and this reasons about a project's own purity promise. The only thing it needs
//! from the pass is the pair the pass already has in hand — a resolved graph to walk and the raw calls
//! the graph was built from.
//!
//! Why the sink evidence is a per-CALLER map rather than a node predicate is the rule's own story
//! (`zzop_rules_graph::cache_lane_file_read`'s module doc): an external-crate callee like `std::fs::read`
//! resolves to nothing, the edge is dropped rather than guessed, and so the sink can never BE a node.
//! Reachability needs resolution; a callee name does not.

use std::collections::HashMap;

use zzop_core::callgraph::{RawCall, SymbolGraph};
use zzop_core::Finding;

/// Runs the rule and returns its findings. `raw_calls` is borrowed rather than consumed — the caller
/// still owns them for the sibling rules.
pub(super) fn run(
    raw_calls: &[RawCall],
    symbol_graph: &SymbolGraph,
    all_symbols: &[zzop_core::SourceSymbol],
    anchor_pattern: Option<&str>,
    file_read_callees: &[&str],
) -> Vec<Finding> {
    let mut call_sites: zzop_rules_graph::CacheLaneCallSites = HashMap::new();
    for c in raw_calls {
        call_sites
            .entry(c.from_symbol.as_str())
            .or_default()
            .insert(c.callee_name.as_str());
    }
    zzop_rules_graph::scan_cache_lane_file_read(&zzop_rules_graph::ScanCacheLaneFileReadInput {
        symbols: all_symbols,
        symbol_graph,
        call_sites: &call_sites,
        cache_lane_anchor_pattern: anchor_pattern,
        file_read_callees,
    })
}
