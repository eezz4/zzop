//! The top-level `exclude` (`zzop_core::RuleConfig::global_excludes`) applied to the SCORE channels, at
//! the emission step only.
//!
//! ## Why this module exists
//! `exclude` is one user statement — "do not report on these paths" — but it originally reached only the
//! channel spelled `findings`. `recommendations` was wired in 2026-07-29 (see
//! [`crate::recommendations`]'s module doc for the measured false headline that forced it); this module
//! closes the last two: `critical` (the summary's `architecture.criticalTop`) and the per-metric violation
//! lists under `scores.*`. Before it, two fields inside the SAME `architecture` object answered in
//! opposite directions — measured on this repo with `exclude: ["crates/core/**"]`, all three `criticalTop`
//! slots were `crates/core/...` while `topRecommendation` in the same run honoured the exclusion.
//!
//! ## Computation is never filtered — only emission
//! Blast radius, every `scores.*.score`, and `health.pain` are computed over the WHOLE graph, before
//! anything here runs. An excluded file is still a real importer and still a real line of code; dropping it
//! from the computation would CORRUPT the metric rather than filter the report, and would make the number
//! incomparable with any other run. So this module only ever shortens an already-computed list — it takes
//! `&mut Scores` after the fact rather than participating in `compute_scores`, and it is applied after
//! `compute_health_index` has already read the scores.
//!
//! Two channels deliberately do NOT get this filter:
//! - `health.pain` — a whole-tree rollup, for the reason above. It reads only `.score` fields, never the
//!   lists this module touches, so the two are independent even if the call order ever moved.
//! - `warnings` — the config-diagnostics channel that carries the "your `exclude` is so broad the problem
//!   only LOOKS absent" tripwire. Filtering it would let the filter erase its own warning.
//!
//! ## What counts as a path
//! Only FILE paths. Rows keyed by a slice or module (`cohesion.slices`, `sdp.violations`,
//! `main_sequence.modules`) are left alone: their key is a directory/slice identifier, not a file the user
//! excluded, and each such row is itself a whole-directory rollup — the same reason `pain` is not filtered.
//! A row naming several files (an import edge, a diamond) is dropped when ANY of its file paths is
//! excluded: the row would otherwise still print the excluded path.

use zzop_core::{global_exclude_matches_path, GlobalExclude};

use crate::scores::types::Scores;

/// True when the config's top-level `exclude` covers `path`. Rule-agnostic, exactly as it is for findings
/// (`zzop_core::is_suppressed`'s `global_excludes` arm), and evaluated with the SAME matcher — never a
/// second glob dialect local to the score channels.
pub fn path_excluded(excludes: &[GlobalExclude], path: &str) -> bool {
    excludes
        .iter()
        .any(|entry| global_exclude_matches_path(entry, path))
}

/// Drops every already-computed `scores.*` violation/detail row that names an excluded file path. Scores
/// themselves (`.score`, and every count/denominator behind it) are untouched — see the module doc.
///
/// Call this AFTER `compute_health_index`, so `pain` is provably derived from the unfiltered rollup.
pub fn apply_excludes_to_scores(scores: &mut Scores, excludes: &[GlobalExclude]) {
    if excludes.is_empty() {
        return;
    }
    let keep = |path: &str| !path_excluded(excludes, path);

    // Edge-shaped rows: both endpoints are dep-graph node ids (file paths).
    scores
        .fsd
        .violations
        .retain(|v| keep(&v.from) && keep(&v.to));
    scores
        .hierarchy
        .violations
        .retain(|v| keep(&v.from) && keep(&v.to));
    scores
        .public_api
        .deep_imports
        .retain(|v| keep(&v.from) && keep(&v.to));
    scores
        .sibling_cross
        .violations
        .retain(|v| keep(&v.from) && keep(&v.to));
    scores
        .diamond
        .pairs
        .retain(|p| keep(&p.root) && keep(&p.leaf) && p.through.iter().all(|t| keep(t)));

    // Single-file rows.
    scores.sfc.violations.retain(|v| keep(&v.path));
    scores.god_file.files.retain(|v| keep(&v.path));
    scores.rename_instability.files.retain(|v| keep(&v.path));
    scores.bus_factor.files.retain(|v| keep(&v.path));
    scores.type_safety.violations.retain(|v| keep(&v.path));
    scores.lod.violations.retain(|v| keep(&v.path));
}

#[cfg(test)]
mod tests;
