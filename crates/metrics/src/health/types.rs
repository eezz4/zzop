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

/// What KIND of judgment a metric makes — the axis a reader needs before quoting `pain` at anyone.
///
/// This exists because the composite folds three different kinds of claim into one number, and until
/// 2026-08-12 nothing anywhere recorded which was which. Measured that day: **80.6% of the weight is
/// [`Opinion`](HealthAxis::Opinion)**, and rule findings contribute **nothing at all** — adding 20 files
/// with `$queryRawUnsafe` to a tree moved `critical` from 1 to 21 and left `pain` byte-identical at 34.4.
/// A reader who takes `pain` for a defect signal is not misreading a subtle document; they are reading a
/// number that has never contained a defect.
///
/// The axis is stored HERE, in the weight table, rather than in prose, so it is one fact with one owner
/// and a new metric cannot ship without declaring which kind of claim it makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthAxis {
    /// Wrong regardless of taste — no competent team chooses this shape on purpose.
    ///
    /// Exactly one metric qualifies today, and it is the one judgment call in this table: `circular`.
    /// An import cycle is not a preference the way a barrel is, but "cycles are categorically bad" is
    /// still a position, not a theorem. It sits here rather than in `Opinion` because that is the
    /// CHARITABLE placement — moving it would take the opinion share from 80.6% to 96.8%, so no
    /// argument about it can rescue the composite.
    Defect,
    /// A school of thought about structure: barrel discipline, FSD layering, Robert Martin's SDP and
    /// Main Sequence, Newman modularity, LOC ceilings. A project that deliberately does the opposite is
    /// not wrong; it scores low. `diamond` has said so in its own description all along (*"A diamond is
    /// not automatically a defect"*) — this axis is that sentence, applied to the eleven that were
    /// silent about it.
    Opinion,
    /// Neither: a property of the repository's HISTORY and the people in it, not of the code's shape.
    /// Needs git, and says nothing about whether the code is correct or well-structured.
    History,
}

/// Per-metric weight and axis for the composite, in iteration order (higher weight = this metric hurts
/// the structure more). The third column is the [`HealthAxis`] — see that type for why it exists and
/// what the columns measured on 2026-08-12.
pub const HEALTH_METRIC_WEIGHTS: &[(HealthMetric, f64, HealthAxis)] = &[
    (HealthMetric::Circular, 3.0, HealthAxis::Defect),
    (HealthMetric::FeatureSlicedDesign, 2.5, HealthAxis::Opinion),
    (HealthMetric::PublicApi, 2.0, HealthAxis::Opinion),
    (HealthMetric::Hierarchy, 2.0, HealthAxis::Opinion),
    (HealthMetric::Sdp, 2.0, HealthAxis::Opinion),
    (HealthMetric::SiblingCross, 1.5, HealthAxis::Opinion),
    (HealthMetric::GodFile, 1.5, HealthAxis::Opinion),
    (HealthMetric::FileSizeCompliance, 1.0, HealthAxis::Opinion),
    (HealthMetric::Diamond, 1.0, HealthAxis::Opinion),
    (HealthMetric::MainSequence, 0.5, HealthAxis::Opinion),
    (HealthMetric::Modularity, 0.5, HealthAxis::Opinion),
    (HealthMetric::Cohesion, 0.5, HealthAxis::Opinion),
    (HealthMetric::RenameInstability, 0.3, HealthAxis::History),
    (HealthMetric::BusFactor, 0.3, HealthAxis::History),
];

/// The sum of every weight in [`HEALTH_METRIC_WEIGHTS`] — the renormalization numerator, and therefore the
/// fixed scale `pain` is expressed on (`0 ..= TOTAL_HEALTH_WEIGHT x 10`, i.e. 0..186) no matter how many
/// axes a given tree could measure. Derived from the table rather than written down, so re-weighting a
/// metric cannot leave a stale constant behind.
pub fn total_health_weight() -> f64 {
    HEALTH_METRIC_WEIGHTS.iter().map(|&(_, w, _)| w).sum()
}

/// The share of the full weight table sitting on one axis — the derivation behind the "80.6% opinion"
/// claim, exported so a doc, a test or a caller states it by RUNNING it instead of copying the number.
pub fn axis_weight(axis: HealthAxis) -> f64 {
    HEALTH_METRIC_WEIGHTS
        .iter()
        .filter(|&&(_, _, a)| a == axis)
        .map(|&(_, w, _)| w)
        .sum()
}

/// One axis's share of `pain` — see [`HealthIndex::axis_pain`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxisPain {
    pub axis: HealthAxis,
    /// This axis's contribution to `pain`, on `pain`'s scale. `0.0` here is a real measurement ("this
    /// axis was judged and found nothing"), unlike `pain`'s `null`.
    pub pain: f64,
    /// The weight this axis carries in the FULL table (`axis_weight`) — the reader's answer to "how much
    /// of the composite could this axis ever have been". `defect` is 3.0 of 18.6; `opinion` is 15.0.
    pub total_weight: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthContributor {
    pub metric: HealthMetric,
    /// Which KIND of claim this row makes — see [`HealthAxis`]. Rides every contributor so a reader
    /// ranking the "why" can tell a structural preference from a defect without a lookup table.
    pub axis: HealthAxis,
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
    /// `pain` split by [`HealthAxis`], on `pain`'s OWN scale, in table order (`defect`, `opinion`,
    /// `history`). Each entry is that axis's share of the same renormalized total, so **the three sum to
    /// `pain`** — that identity is what makes them quotable without a second denominator, and
    /// `axis_shares_sum_to_pain` pins it.
    ///
    /// This is the field the composite was missing. `pain` alone cannot answer "is this repository
    /// broken, or does it just disagree with me about barrels", and that question is the only reason
    /// most readers open the number. `null` exactly when `pain` is null.
    pub axis_pain: Option<Vec<AxisPain>>,
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
