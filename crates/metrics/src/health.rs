//! Composite structural-health ("pain") index — rolls the per-metric structural `Scores` up into ONE number.
//!
//! Why this lives in the engine: every consumer (the CLI report, the JSON output, any dashboard) wants a single
//! "how bad is this service" scalar and a ranked "why". Without it each consumer re-derives its own rollup from the
//! 14 scores (a dashboard was doing exactly that) — and they disagree. Emitting it here makes the engine the SSOT.
//!
//! # Formula (renormalized over the MEASURED metrics only — 2026-08-08)
//!
//! Each MEASURED metric contributes `weight x gap x 10`, where `gap = (100 - score) / 100` (how far below
//! perfect). `circular` is binary — any import cycle scores its full weight (cycles are categorically bad,
//! not a matter of degree). Those raw points are then scaled back onto the full weight table:
//!
//! ```text
//! measured(m)     = population(m) > 0
//! raw             = SUM over measured m of  weight(m) x gap(m) x 10
//! measuredWeight  = SUM over measured m of  weight(m)
//! pain            = raw x TOTAL_HEALTH_WEIGHT / measuredWeight        (measuredWeight > 0)
//! pain            = null                                              (measuredWeight == 0)
//! ```
//!
//! ## What this replaced, and why it was wrong
//!
//! The old formula was `pain = SUM over ALL 14 metrics of weight x gap x 10`, with contributors filtered to
//! `contribution > 0.0`. Every per-metric score returns 100 when its population is empty (`if total == 0 {
//! 100.0 }` — the guard appears in a dozen score modules), so a metric that had NOTHING TO MEASURE produced
//! `gap 0`, contributed 0 pain, and was then dropped from `contributors` — byte-identical to a metric that
//! scored 100 because the code is clean.
//!
//! **The consequence was backwards: an unmeasurable axis made a repo look HEALTHIER.** Measured on a Go
//! tree, `featureSlicedDesign` (weight 2.5, the second highest) is defined over a directory convention Go
//! does not use, so it silently removed 25 points of possible pain from the total while every other metric's
//! contribution stayed put. A tree that zzop could see LESS of scored better than one it could see fully.
//!
//! Renormalizing fixes the direction: an unmeasured metric leaves the weighting entirely rather than
//! passing it, so `pain` always describes the axes that were actually judged, on a stable 0..186 scale that
//! stays comparable across trees measuring different numbers of axes. The scale is preserved on purpose —
//! `pain` is quoted between runs and against other repos, so the fix must not silently re-base it.
//!
//! ## `pain` is `Option<f64>`, and `contributors` keeps its dark rows
//!
//! `null` (not `0.0`) when NOTHING was measurable: there is no population to renormalize over, and `0.0`
//! there would be the strongest possible false all-clear. Same `null`-vs-`[]` convention `co_change`
//! already uses — absence of data is not a measurement of zero.
//!
//! `contributors` no longer drops a metric just because it contributed nothing. A metric with
//! `population: 0` rides the list with `gap: null` / `contribution: null`, so the reply STATES which axes
//! were dark instead of leaving the reader to notice an absence. Measured-and-clean keeps `Some(0.0)` and
//! is still dropped from the ranked "why" — the distinction the old filter erased is exactly the one that
//! survives now.
//!
//! ## Why the population had to reach this file at all
//!
//! `health.pain` is a single composite scalar, and it is the ONLY score number the CLI and MCP surfaces
//! publish (`crates/summary`'s `architecture.pain`; the full `scores` object rides only the direct
//! `zzop-facade` embedding lane). `zzop_facade::query_coverage` forbids exactly this shape one crate over
//! — *"there is deliberately NO single score field, and one must never be added"* (2026-07-31 user ruling)
//! — because a folded number gets quoted without its exclusion list. `pain` cannot stop being a composite;
//! what it can do is carry its own denominator, which is why `measured_weight`/`total_weight` are FIELDS
//! and why `architecture` ships them beside `pain` rather than shipping the scalar alone.

use crate::scores::types::Scores;

mod types;

pub use types::{
    total_health_weight, HealthContributor, HealthIndex, HealthMetric, HEALTH_METRIC_WEIGHTS,
};

/// The 0-100 score scale.
const PERCENT: f64 = 100.0;
/// Each metric contributes `weight x gap x POINTS_PER_GAP` to the composite.
const POINTS_PER_GAP: f64 = 10.0;

/// Rounds to 3 decimal places.
fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// Rounds to 2 decimal places.
fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Rounds to 1 decimal place.
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// `gap(score) = max(0, (100 - score) / 100)`.
fn gap(score: f64) -> f64 {
    ((PERCENT - score) / PERCENT).max(0.0)
}

/// Rolls up `scores` into a composite pain index plus the contributors behind it.
///
/// The POPULATION each metric judged, read off that score's own denominator — never off a second table.
/// A metric whose population is 0 measured nothing and is excluded from the composite (see this module's
/// doc for the renormalization).
///
/// `feature_sliced_design` reads `layer_classified_imports`, NOT its ratio denominator `total_imports`:
/// the question here is "could this metric judge this tree at all", and an undeclared (or wrong-language)
/// FSD vocabulary leaves every path unclassified, which produces a large `total_imports`, zero possible
/// violations, and a perfect score for a repo that never adopted the convention.
///
/// `circular`'s population is `coupling`'s importer count: cycles are found in the resolved import graph,
/// so "no file imports anything" is precisely the state in which "no cycles" is not a finding. Reading it
/// off `coupling` rather than off a cycle count of its own is deliberate — a cycle count is the metric's
/// RESULT, and a result can never be its own denominator (0 cycles would read as "nothing to measure",
/// inverting the meaning of a clean graph).
fn population_of(scores: &Scores, metric: HealthMetric) -> u32 {
    match metric {
        HealthMetric::Circular => scores.coupling.importer_count,
        HealthMetric::FeatureSlicedDesign => scores.feature_sliced_design.layer_classified_imports,
        HealthMetric::PublicApi => scores.public_api.total_cross_module_imports,
        HealthMetric::Hierarchy => scores.hierarchy.total_intra_module_edges,
        HealthMetric::Sdp => scores.sdp.total_cross_slice_edges,
        HealthMetric::SiblingCross => scores.sibling_cross.total_intra_module_edges,
        HealthMetric::GodFile => scores.god_file.total,
        HealthMetric::FileSizeCompliance => scores.file_size_compliance.total,
        HealthMetric::Diamond => scores.diamond.roots_examined,
        HealthMetric::MainSequence => scores.main_sequence.classified_files,
        HealthMetric::Modularity => scores.modularity.edge_count,
        HealthMetric::Cohesion => scores.cohesion.slice_count,
        HealthMetric::RenameInstability => scores.rename_instability.total,
        HealthMetric::BusFactor => scores.bus_factor.total,
    }
}

/// Rolls up `scores` into a single composite pain index plus the ranked, non-zero contributors behind it.
pub fn compute_health_index(scores: &Scores) -> HealthIndex {
    let gap_of = |metric: HealthMetric| -> f64 {
        match metric {
            HealthMetric::Circular => {
                if scores.coupling.circular_count > 0 {
                    1.0
                } else {
                    0.0
                }
            }
            HealthMetric::FeatureSlicedDesign => gap(scores.feature_sliced_design.score),
            HealthMetric::PublicApi => gap(scores.public_api.score),
            HealthMetric::Hierarchy => gap(scores.hierarchy.score),
            HealthMetric::Sdp => gap(scores.sdp.score),
            HealthMetric::SiblingCross => gap(scores.sibling_cross.score),
            HealthMetric::GodFile => gap(scores.god_file.score),
            HealthMetric::FileSizeCompliance => gap(scores.file_size_compliance.score),
            HealthMetric::Diamond => gap(scores.diamond.score),
            HealthMetric::MainSequence => gap(scores.main_sequence.score),
            HealthMetric::Modularity => gap(scores.modularity.score),
            HealthMetric::Cohesion => gap(scores.cohesion.score),
            HealthMetric::RenameInstability => gap(scores.rename_instability.score),
            HealthMetric::BusFactor => gap(scores.bus_factor.score),
        }
    };

    let all: Vec<HealthContributor> = HEALTH_METRIC_WEIGHTS
        .iter()
        .map(|&(metric, weight)| {
            let population = population_of(scores, metric);
            // A population of 0 is the "never measured" signal — no gap, no contribution, and no seat
            // in the weighting below. Reporting `gap: 0.0` here is what used to make an unmeasurable
            // axis indistinguishable from a clean one.
            let (gap_v, contribution) = if population == 0 {
                (None, None)
            } else {
                let g = gap_of(metric);
                (Some(round3(g)), Some(round2(weight * g * POINTS_PER_GAP)))
            };
            HealthContributor {
                metric,
                weight,
                population,
                gap: gap_v,
                contribution,
            }
        })
        .collect();

    // Only measured metrics carry weight — this is the renormalization denominator.
    let measured_weight: f64 = all
        .iter()
        .filter(|c| c.population > 0)
        .map(|c| c.weight)
        .sum();
    let raw: f64 = all.iter().filter_map(|c| c.contribution).sum();
    let total_weight = total_health_weight();
    // Scale the measured subset's raw points back onto the full table, so losing an axis can never lower
    // `pain`. `measured_weight == 0` means nothing was judged at all: absence of data, not 0 pain.
    let pain = (measured_weight > 0.0).then(|| round1(raw * total_weight / measured_weight));

    // The ranked "why" keeps only metrics that explain something; the UNMEASURED ones follow, because
    // "this axis was dark" is itself a thing the reader must be told rather than left to infer from an
    // absence. Measured-and-clean (contribution 0.0) is dropped — it is neither a driver nor a gap.
    let mut contributors: Vec<HealthContributor> = all
        .iter()
        .copied()
        .filter(|c| c.contribution.is_some_and(|v| v > 0.0))
        .collect();
    contributors.sort_by(|a, b| {
        b.contribution
            .partial_cmp(&a.contribution)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    contributors.extend(all.iter().copied().filter(|c| c.population == 0));

    HealthIndex {
        pain,
        measured_weight,
        total_weight,
        contributors,
    }
}

#[cfg(test)]
mod tests;
