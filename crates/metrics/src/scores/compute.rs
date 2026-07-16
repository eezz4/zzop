//! Score orchestrator — fans out to all 17 score functions and assembles the aggregate `Scores` struct.
//! All scores are 0-100; higher is better.

use std::collections::HashMap;

use zzop_core::{DepGraph, FileNode};

use super::config::ScoresConfig;
use super::lod::LodChain;
use super::type_safety::TypeSafetyCounts;
use super::types::{FileKinds, Scores};
use super::{
    bus_factor, cohesion, coupling, diamond, fix_ratio, fsd, god_file, hierarchy, lod,
    main_sequence, modularity, public_api, rename, sdp, sfc, sibling_cross, type_safety,
};

/// Inputs to [`compute_scores`]. Optional inputs have no ambient defaulting: callers that don't have
/// `file_kinds`, `type_safety_counts`, or `lod_by_file` data pass an empty collection explicitly.
pub struct ScoresInput<'a> {
    pub nodes: &'a [FileNode],
    pub dep: &'a DepGraph,
    pub circular: &'a [Vec<String>],
    pub target: Option<&'a str>,
    pub file_kinds: &'a FileKinds,
    pub type_safety_counts: &'a HashMap<String, TypeSafetyCounts>,
    pub lod_by_file: &'a HashMap<String, Vec<LodChain>>,
    /// Source-ness classifier for the LOC-size-based `sfc`/`god_file` metrics — the same "no ambient
    /// defaulting" contract as `file_kinds`/`type_safety_counts`/`lod_by_file`: callers without real
    /// classification data pass a closure explicitly (e.g. `&|_| true` when every node is known source).
    /// A `&dyn Fn` reference (not a second generic parameter on `ScoresInput`/`compute_scores`) keeps the
    /// orchestrator's signature simple — `&'_ dyn Fn(&str) -> bool` itself satisfies `compute_sfc`/
    /// `compute_god_file`'s `F: Fn(&str) -> bool` bound directly. Only those two metrics use raw `loc` as a
    /// violation-selection criterion across every live node; other `loc` readers gate on liveness or on a
    /// separately-populated per-file map, so they need no source-ness gate.
    pub is_source: &'a dyn Fn(&str) -> bool,
}

/// Assembles the full `Scores` report by calling each of the 17 metric modules exactly once. `coupling` receives
/// `circular.len()` (the cycle count), never the cycles themselves.
pub fn compute_scores(input: &ScoresInput, cfg: &ScoresConfig) -> Scores {
    Scores {
        fsd: fsd::compute_fsd(input.dep, cfg),
        cohesion: cohesion::compute_cohesion(input.dep, cfg),
        coupling: coupling::compute_coupling(input.nodes, input.circular.len(), cfg),
        sdp: sdp::compute_sdp(input.dep, cfg),
        hierarchy: hierarchy::compute_hierarchy(input.dep, cfg),
        public_api: public_api::compute_public_api(input.dep, cfg),
        sfc: sfc::compute_sfc(input.nodes, input.target, cfg, input.is_source),
        main_sequence: main_sequence::compute_main_sequence(input.dep, input.file_kinds, cfg),
        modularity: modularity::compute_modularity(input.dep, cfg),
        god_file: god_file::compute_god_file(input.nodes, input.target, cfg, input.is_source),
        sibling_cross: sibling_cross::compute_sibling_cross(input.dep, cfg),
        diamond: diamond::compute_diamond(input.dep, cfg),
        rename_instability: rename::compute_rename(input.nodes),
        bus_factor: bus_factor::compute_bus_factor(input.nodes, cfg),
        fix_ratio: fix_ratio::compute_fix_ratio(input.nodes, cfg),
        type_safety: type_safety::compute_type_safety(input.nodes, input.type_safety_counts, cfg),
        lod: lod::compute_lod(input.nodes, input.lod_by_file, cfg),
    }
}

#[cfg(test)]
mod tests;
