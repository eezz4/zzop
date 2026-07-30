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
    /// False when the config's top-level `exclude` (`zzop_core::RuleConfig::global_excludes`) covers this
    /// path — "do not JUDGE this file", which is not the same statement as "this file does not exist".
    ///
    /// An excluded file stays a real node with real edges, so every fact ABOUT ANOTHER FILE that depends
    /// on it is unchanged: a source file importing an excluded vendor SDK keeps that fan-out, keeps that
    /// coupling, and keeps the layer violation the import commits — the importing file made that choice
    /// and is not excluded. What the gate removes is the excluded file's OWN standing as a subject: it is
    /// counted in neither the violation list nor the denominator behind the score, exactly as `is_source`
    /// already does for non-source files (see [`super::god_file`]'s module doc for why leaving it in the
    /// denominator-only would silently inflate the compliant ratio instead of filtering the report).
    ///
    /// The four SLICE/MODULE-keyed metrics (`cohesion`, `sdp`, `main_sequence`, `modularity`) never
    /// consult this: their subject is a directory rollup, not a file, so "the excluded file's own
    /// standing" has no referent there. That is the same boundary `crate::report_excludes` already draws
    /// for the emitted lists, kept identical so the counted set and the printed set cannot disagree.
    ///
    /// Callers with no excludes pass `&|_| true`, the same "no ambient defaulting" contract as the fields
    /// above.
    pub is_scored: &'a dyn Fn(&str) -> bool,
}

/// Assembles the full `Scores` report by calling each of the 17 metric modules exactly once. `coupling`
/// receives `circular.len()` (the cycle count), never the cycles themselves.
///
/// Thirteen take `input.is_scored` — see [`ScoresInput::is_scored`] for what the gate means and why it
/// applies to both the violation list and the denominator. The four that do not (`cohesion`, `sdp`,
/// `main_sequence`, `modularity`) are keyed by slice/module rather than by file.
pub fn compute_scores(input: &ScoresInput, cfg: &ScoresConfig) -> Scores {
    Scores {
        fsd: fsd::compute_fsd(input.dep, cfg, input.is_scored),
        cohesion: cohesion::compute_cohesion(input.dep, cfg),
        coupling: coupling::compute_coupling(
            input.nodes,
            input.circular.len(),
            cfg,
            input.is_scored,
        ),
        sdp: sdp::compute_sdp(input.dep, cfg),
        hierarchy: hierarchy::compute_hierarchy(input.dep, cfg, input.is_scored),
        public_api: public_api::compute_public_api(input.dep, cfg, input.is_scored),
        sfc: sfc::compute_sfc(
            input.nodes,
            input.target,
            cfg,
            input.is_source,
            input.is_scored,
        ),
        main_sequence: main_sequence::compute_main_sequence(input.dep, input.file_kinds, cfg),
        modularity: modularity::compute_modularity(input.dep, cfg),
        god_file: god_file::compute_god_file(
            input.nodes,
            input.target,
            cfg,
            input.is_source,
            input.is_scored,
        ),
        sibling_cross: sibling_cross::compute_sibling_cross(input.dep, cfg, input.is_scored),
        diamond: diamond::compute_diamond(input.dep, cfg, input.is_scored),
        rename_instability: rename::compute_rename(input.nodes, input.is_scored),
        bus_factor: bus_factor::compute_bus_factor(input.nodes, cfg, input.is_scored),
        fix_ratio: fix_ratio::compute_fix_ratio(input.nodes, cfg, input.is_scored),
        type_safety: type_safety::compute_type_safety(
            input.nodes,
            input.type_safety_counts,
            cfg,
            input.is_scored,
        ),
        lod: lod::compute_lod(input.nodes, input.lod_by_file, cfg, input.is_scored),
    }
}

#[cfg(test)]
mod tests;
