//! Score orchestrator — fans out to all 15 score functions and assembles the aggregate `Scores` struct.
//! All scores are 0-100; higher is better, and every one of them ships the POPULATION it scored over —
//! see [`super::types::Scores`] for the per-metric SUBJECT counted and why a score without one is not a
//! measurement. Which FIELD carries each count is held by machines only (`crate::health::population_of`
//! and `scores::meanings::tests`' `POPULATION_FIELD`) — a prose copy of that mapping drifted once.
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
/// ELEVEN of the fifteen take the per-file gate [`ScoresInput::is_scored`] — see that field for what the
/// gate means and why it applies to both the violation list and the denominator. The four that do not
/// (`cohesion`, `sdp`, `main_sequence`, `modularity`) are keyed by slice/module rather than by file.
/// (This sentence read "thirteen" until 2026-09-11, which no arithmetic in this file supports — 15 minus
/// the 4 named right after it is 11. Recount without trusting either number:
/// `sed -n '/^    Scores {/,/^    }$/p' crates/metrics/src/scores/compute.rs | grep -c is_scored`.)
///
/// # The scored POPULATION is applied here, and ONLY to the eleven file-keyed metrics
/// [`ScoresConfig::population`] says which files may be counted at all, and it reaches the scores by
/// exactly one route: it folds into the `is_scored` gate. An excluded file loses its own standing as a
/// judged subject and nothing else — every OTHER file's fan-out, coupling and violations are untouched,
/// exactly as [`ScoresInput::is_scored`] already specifies for the top-level `exclude` key.
///
/// **The four slice/module-keyed metrics therefore keep scoring the WHOLE tree**, test files included,
/// while their eleven siblings score a narrowed one. That is a real split inside one `Scores` struct,
/// and it is PUBLISHED rather than left for a reader to infer: the shaped reply's
/// `architecture.painMeaning` names those four by name whenever the key is on (`crates/summary/src/
/// analyze/architecture.rs`, pinned end to end by `crates/summary/tests/
/// score_population_disclosure.rs`). It is stated there rather than in `super::meanings` because the
/// `scoreMeanings` WIRE FIELD was deleted on 2026-09-06 for having no reader — a sentence added there
/// would reach nobody, which is the failure this comment would otherwise be committing.
///
/// ## Why they are not narrowed too (2026-09-11 user ruling; the first attempt DID narrow them)
/// The first implementation handed those four a dep graph RESTRICTED to the population — an excluded
/// file was neither a member of its slice nor an endpoint of any counted edge. It was reverted, and the
/// reasoning is measurement rather than taste:
///
/// * The metric it was built for measures NOTHING on any tree anyone has run. `cohesion` is the only
///   score whose subject is an FSD SLICE, and its own published population (`cohesion.sliceCount`) is
///   0 on all 21 corpus trees AND on this repo: the slice regex is anchored (`^<container>/<slice>/`),
///   so a tree whose slices sit under `src/` or `public/app/` has none at all. A restriction whose
///   only slice-keyed beneficiary has an empty population is buying nothing.
/// * It costs a doctrine that holds everywhere. Dropping a node from the graph states that a real
///   dependency does not exist, which [`ScoresInput::is_scored`] exists to avoid. Trading a rule that
///   holds on every tree for a delta that shows up on none is the wrong side of that exchange.
///
/// **What the revert costs, measured rather than assumed**: the other three (`sdp`, `main_sequence`,
/// `modularity`) are FOLDER-keyed (`super::shared::module_of`, which falls back to the top path
/// segment), so they have real populations — and with the restriction gone they no longer respond to
/// the key AT ALL. Measured over the 21-tree corpus (`corpus/frameworks/*`, each analyzed twice, the
/// key off then on): `main_sequence` moved on 14 trees WITH the restriction and on **0** without it;
/// `modularity` 14 -> 0; `sdp` 2 -> 0; `cohesion` 0 -> 0. That is the price, it is not zero, and it is
/// recorded here so re-opening this is an argument about a known number rather than a rediscovery.
///
/// When the filter keeps everything — the DEFAULT — the wrapping does not run at all: the caller's own
/// `is_scored` reference is passed through, so the untouched path is untouched rather than merely
/// equivalent.
pub fn compute_scores(input: &ScoresInput, cfg: &ScoresConfig) -> Scores {
    let population_scoped = |path: &str| (input.is_scored)(path) && !cfg.population.excludes(path);
    // The caller's own `&dyn Fn` when nothing is excluded, so the default path is the pre-existing one
    // by reference rather than by arithmetic.
    let is_scored: &dyn Fn(&str) -> bool = if cfg.population.keeps_everything() {
        input.is_scored
    } else {
        &population_scoped
    };
    Scores {
        feature_sliced_design: feature_sliced_design::compute_feature_sliced_design(
            input.dep, cfg, is_scored,
        ),
        cohesion: cohesion::compute_cohesion(input.dep, cfg),
        coupling: coupling::compute_coupling(input.nodes, input.circular.len(), cfg, is_scored),
        sdp: sdp::compute_sdp(input.dep, cfg),
        hierarchy: hierarchy::compute_hierarchy(input.dep, cfg, is_scored),
        public_api: public_api::compute_public_api(input.dep, cfg, is_scored),
        file_size_compliance: file_size_compliance::compute_file_size_compliance(
            input.nodes,
            input.target,
            cfg,
            input.is_source,
            is_scored,
        ),
        main_sequence: main_sequence::compute_main_sequence(input.dep, input.file_kinds, cfg),
        modularity: modularity::compute_modularity(input.dep, cfg),
        god_file: god_file::compute_god_file(
            input.nodes,
            input.target,
            cfg,
            input.is_source,
            is_scored,
        ),
        sibling_cross: sibling_cross::compute_sibling_cross(input.dep, cfg, is_scored),
        diamond: diamond::compute_diamond(input.dep, cfg, is_scored),
        rename_instability: rename::compute_rename(input.nodes, is_scored),
        bus_factor: bus_factor::compute_bus_factor(input.nodes, cfg, is_scored),
        fix_ratio: fix_ratio::compute_fix_ratio(input.nodes, cfg, is_scored),
    }
}

#[cfg(test)]
mod tests;
