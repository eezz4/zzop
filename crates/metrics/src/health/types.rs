//! The health index's wire TYPES — the weighted metric table, one contributor row, and the rolled-up
//! [`HealthIndex`] itself.
//!
//! Split out of the parent `health.rs` on 2026-08-08 to stay under the per-file line cap, when the
//! population/renormalization contract landed. The parent keeps the COMPUTATION and the reasoning
//! behind the formula; this file is the shape that reasoning produces.

use serde::{Deserialize, Serialize};

/// One of the 14 structural metrics rolled into the composite pain index. Serializes to camelCase so
/// JSON output/reporting field names stay consistent with this crate's other output types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthMetric {
    Circular,
    FeatureSlicedDesign,
    PublicApi,
    Hierarchy,
    Sdp,
    SiblingCross,
    GodFile,
    FileSizeCompliance,
    Diamond,
    MainSequence,
    Modularity,
    Cohesion,
    RenameInstability,
    BusFactor,
}

/// Per-metric weight for the composite, in `HEALTH_METRIC_WEIGHTS` iteration order (higher = this metric
/// hurts the structure more).
pub const HEALTH_METRIC_WEIGHTS: &[(HealthMetric, f64)] = &[
    (HealthMetric::Circular, 3.0),
    (HealthMetric::FeatureSlicedDesign, 2.5),
    (HealthMetric::PublicApi, 2.0),
    (HealthMetric::Hierarchy, 2.0),
    (HealthMetric::Sdp, 2.0),
    (HealthMetric::SiblingCross, 1.5),
    (HealthMetric::GodFile, 1.5),
    (HealthMetric::FileSizeCompliance, 1.0),
    (HealthMetric::Diamond, 1.0),
    (HealthMetric::MainSequence, 0.5),
    (HealthMetric::Modularity, 0.5),
    (HealthMetric::Cohesion, 0.5),
    (HealthMetric::RenameInstability, 0.3),
    (HealthMetric::BusFactor, 0.3),
];

/// The sum of every weight in [`HEALTH_METRIC_WEIGHTS`] — the renormalization numerator, and therefore the
/// fixed scale `pain` is expressed on (`0 ..= TOTAL_HEALTH_WEIGHT x 10`, i.e. 0..186) no matter how many
/// axes a given tree could measure. Derived from the table rather than written down, so re-weighting a
/// metric cannot leave a stale constant behind.
pub fn total_health_weight() -> f64 {
    HEALTH_METRIC_WEIGHTS.iter().map(|&(_, w)| w).sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthContributor {
    pub metric: HealthMetric,
    pub weight: f64,
    /// POPULATION: how many subjects this metric actually judged, read off the score's own denominator
    /// (see [`crate::scores::types::Scores`] for the per-metric field this comes from). `0` is the
    /// "never measured" signal and is what removes the metric from the composite — no separate flag,
    /// because a flag would be a second owner of the same fact.
    pub population: u32,
    /// Normalized shortfall 0-1 (how far below a perfect 100). `circular`: 1 if any cycle exists, else 0.
    /// `None` when `population` is 0 — there is no shortfall to report against an empty population, and
    /// the `0.0` this used to ship was indistinguishable from a perfect score.
    pub gap: Option<f64>,
    /// `weight x gap x 10` — this metric's RAW points, before the renormalization that produces `pain`
    /// (so the contributions do not sum to `pain` unless every metric was measured; `measured_weight`
    /// beside them is the scale factor). `None` when `population` is 0.
    pub contribution: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthIndex {
    /// Composite structural debt over the MEASURED metrics, renormalized onto the full weight table —
    /// higher = worse, 0 = every measured structural score is at 100 and no cycles.
    ///
    /// `None` when `measured_weight` is 0: not one of the fourteen metrics judged a single subject, so
    /// there is no population to compute a composite over. Absence of data, never `0.0` — a zero here
    /// would be the strongest false all-clear this output can emit.
    pub pain: Option<f64>,
    /// The weight actually behind `pain` — the sum of `weight` over metrics with a non-zero population.
    /// Read `pain` against this: at `measured_weight == total_weight` every axis was judged, and the
    /// further below it sits the fewer axes the number describes. This is the denominator that makes
    /// `pain` quotable; without it the scalar is exactly the folded single score
    /// `zzop_facade::query_coverage` forbids.
    pub measured_weight: f64,
    /// [`total_health_weight`] — the full table, so a consumer can form `measured_weight / total_weight`
    /// without carrying a copy of the weights.
    pub total_weight: f64,
    /// Metrics driving the number, highest contribution first, followed by every UNMEASURED metric
    /// (`population: 0`, `gap: null`). Measured-but-clean metrics are dropped — they explain nothing —
    /// while unmeasured ones are kept, because "this axis was dark" is the fact the old zero-filter
    /// destroyed.
    pub contributors: Vec<HealthContributor>,
}
