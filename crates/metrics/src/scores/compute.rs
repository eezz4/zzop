//! Score orchestrator — fans out to all 15 score functions and assembles the aggregate `Scores` struct.
//! All scores are 0-100; higher is better, and every one of them ships the POPULATION it scored over —
//! see [`super::types::Scores`] for the per-metric population field and why a score without one is not a
//! measurement.
//!
//! Two metrics left this fan-out on 2026-08-08 (`type_safety`, `lod`): their input channels had no
//! producer anywhere in the workspace, so both published a perfect score on every run ever made. The
//! `Scores` doc has the census and the reasoning.

use zzop_core::{DepGraph, FileNode};

use super::config::ScoresConfig;
use super::types::{FileKinds, Scores};
use super::{
    bus_factor, cohesion, coupling, diamond, feature_sliced_design, file_size_compliance,
    fix_ratio, god_file, hierarchy, main_sequence, modularity, public_api, rename, sdp,
    sibling_cross,
};

/// Inputs to [`compute_scores`]. Optional inputs have no ambient defaulting: a caller that doesn't have
/// `file_kinds` data passes an empty collection explicitly.
pub struct ScoresInput<'a> {
    pub nodes: &'a [FileNode],
    pub dep: &'a DepGraph,
    pub circular: &'a [Vec<String>],
    pub target: Option<&'a str>,
    /// Per-file abstract/concrete classification, consumed only by `main_sequence`'s abstractness term.
    ///
    /// **No production caller can populate this**: nothing in the workspace classifies a file's kind, so
    /// the only production call site passes it empty and `main_sequence` reports `classified_files: 0`
    /// to say so on the wire. The parameter stays because it is the seam a classifier plugs into, and
    /// because the metric's OTHER input (instability, from the real dep graph) is live — see
    /// [`super::types::MainSequenceScore::classified_files`] for why that made deletion the wrong call
    /// here while it was the right one for `type_safety`/`lod`.
    pub file_kinds: &'a FileKinds,
    /// Source-ness classifier for the LOC-size-based `file_size_compliance`/`god_file` metrics — the same "no ambient
    /// defaulting" contract as `file_kinds`: callers without real
    /// classification data pass a closure explicitly (e.g. `&|_| true` when every node is known source).
    /// A `&dyn Fn` reference (not a second generic parameter on `ScoresInput`/`compute_scores`) keeps the
    /// orchestrator's signature simple — `&'_ dyn Fn(&str) -> bool` itself satisfies `compute_file_size_compliance`/
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

/// Assembles the full `Scores` report by calling each of the 15 metric modules exactly once. `coupling`
/// receives `circular.len()` (the cycle count), never the cycles themselves.
///
/// Thirteen take `input.is_scored` — see [`ScoresInput::is_scored`] for what the gate means and why it
/// applies to both the violation list and the denominator. The four that do not (`cohesion`, `sdp`,
/// `main_sequence`, `modularity`) are keyed by slice/module rather than by file.
pub fn compute_scores(input: &ScoresInput, cfg: &ScoresConfig) -> Scores {
    Scores {
        feature_sliced_design: feature_sliced_design::compute_feature_sliced_design(
            input.dep,
            cfg,
            input.is_scored,
        ),
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
        file_size_compliance: file_size_compliance::compute_file_size_compliance(
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
    }
}

#[cfg(test)]
mod tests;
