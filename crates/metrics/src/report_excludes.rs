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
//! ## What this module does NOT do (revised 2026-07-30)
//! It does not make the scores respond to `exclude`. That happens upstream now, inside `compute_scores`,
//! via [`crate::scores::compute::ScoresInput::is_scored`]: an excluded file stops being a judged SUBJECT
//! and leaves both the violation list and the denominator behind each score, so `health.pain` follows.
//! This module is what remains after that — the rows that reach a list WITHOUT their subject being
//! excluded, most importantly the edge-shaped rows whose subject is a scored importer but whose other
//! endpoint names an excluded path.
//!
//! The distinction is load-bearing, and this doc used to state its opposite ("computation is never
//! filtered — only emission"). What that rule got right is that DELETING an excluded file from the graph
//! would corrupt the metric: it is still a real importer and a real import target, and no scored file's
//! coupling or fan-out may move because reporting stopped. What it got wrong is treating "not in the
//! graph" and "not a subject" as the same operation. Measured on this repo, excluding three whole
//! top-level directories replaced every `criticalTop` slot and left `pain` identical to one decimal — the
//! number stayed while the evidence for it was deleted, which for anyone excluding code they cannot
//! change made `pain` a figure with no available action behind it.
//!
//! Two channels still take no filter at all:
//! - `warnings` — the config-diagnostics channel that carries the "your `exclude` is so broad the problem
//!   only LOOKS absent" tripwire. Filtering it would let the filter erase its own warning.
//! - Anything keyed by a slice or module (see "What counts as a path" below). Those four metrics
//!   (`cohesion`, `sdp`, `main_sequence`, `modularity`) receive no subject gate upstream either, so the
//!   counted set and the printed set cannot disagree about them.
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

/// Drops every already-computed `scores.*` violation/detail row that names an excluded file path.
///
/// The score itself is not recomputed here — it already accounts for the exclusion, because
/// `compute_scores` never counted an excluded file as a subject in the first place
/// ([`crate::scores::compute::ScoresInput::is_scored`]). What this removes is the residue: a row whose
/// SUBJECT is scored but whose other endpoint names an excluded path (an import edge, a diamond). That row
/// IS counted — the scored file made the import and owns the violation, and its `fan_out` counts the same
/// edge — so dropping it would recreate in miniature the very defect this whole seam exists to close: a
/// number with its evidence deleted.
///
/// So the other endpoint is REDACTED to `zzop_core::REDACTED` rather than the row being dropped. That is
/// not a new policy invented here; it is the one `zzop_core::registry::redact` already applies to finding
/// evidence, for the same reason its doc gives — the reader must be able to tell "this run was filtered
/// here" from "there was never a second place". Counted and printed therefore stay the same set.
///
/// Rows whose SUBJECT is excluded do not reach this function at all: `compute_scores` never produced
/// them. The subject-side `retain`s below are kept as a cheap belt-and-braces, not as the mechanism.
///
/// Call order no longer matters for correctness — nothing downstream re-derives a score from these lists.
pub fn apply_excludes_to_scores(scores: &mut Scores, excludes: &[GlobalExclude]) {
    if excludes.is_empty() {
        return;
    }
    let keep = |path: &str| !path_excluded(excludes, path);
    let mask = |path: &mut String| {
        if path_excluded(excludes, path) {
            *path = zzop_core::REDACTED.to_string();
        }
    };

    // Edge-shaped rows: both endpoints are dep-graph node ids (file paths). The `from`/`root` side is the
    // judged subject and is already guaranteed scored; the far side is redacted so the violation keeps its
    // evidence without naming a path the config excluded.
    for v in &mut scores.fsd.violations {
        mask(&mut v.to);
    }
    for v in &mut scores.hierarchy.violations {
        mask(&mut v.to);
    }
    for v in &mut scores.public_api.deep_imports {
        mask(&mut v.to);
    }
    for v in &mut scores.sibling_cross.violations {
        mask(&mut v.to);
    }
    for p in &mut scores.diamond.pairs {
        mask(&mut p.leaf);
        for t in &mut p.through {
            mask(t);
        }
    }

    // Subject-side rows. Unreachable in practice (the subject gate ran upstream), kept so a future caller
    // that skips `is_scored` still cannot print an excluded path.
    scores.fsd.violations.retain(|v| keep(&v.from));
    scores.hierarchy.violations.retain(|v| keep(&v.from));
    scores.public_api.deep_imports.retain(|v| keep(&v.from));
    scores.sibling_cross.violations.retain(|v| keep(&v.from));
    scores.diamond.pairs.retain(|p| keep(&p.root));
    scores.sfc.violations.retain(|v| keep(&v.path));
    scores.god_file.files.retain(|v| keep(&v.path));
    scores.rename_instability.files.retain(|v| keep(&v.path));
    scores.bus_factor.files.retain(|v| keep(&v.path));
    scores.type_safety.violations.retain(|v| keep(&v.path));
    scores.lod.violations.retain(|v| keep(&v.path));
}

#[cfg(test)]
mod tests;
