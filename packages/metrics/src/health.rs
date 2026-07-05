//! Composite structural-health ("pain") index — rolls the per-metric structural `Scores` up into ONE number.
//!
//! Why this lives in the engine: every consumer (the CLI report, the JSON output, any dashboard) wants a single
//! "how bad is this service" scalar and a ranked "why". Without it each consumer re-derives its own rollup from the
//! 14 scores (a dashboard was doing exactly that) — and they disagree. Emitting it here makes the engine the SSOT.
//!
//! Formula: each metric contributes `weight x gap x 10`, where `gap = (100 - score) / 100` (how far below perfect).
//! `circular` is binary — any import cycle scores its full weight (cycles are categorically bad, not a matter of
//! degree). `pain` is the sum; higher = worse, 0 = every weighted structural score is perfect (and no cycles).

use serde::{Deserialize, Serialize};

use crate::scores::types::Scores;

/// The 0-100 score scale.
const PERCENT: f64 = 100.0;
/// Each metric contributes `weight x gap x POINTS_PER_GAP` to the composite.
const POINTS_PER_GAP: f64 = 10.0;

/// One of the 14 structural metrics rolled into the composite pain index. Serializes to camelCase so
/// JSON output/reporting field names stay consistent with this crate's other output types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthMetric {
    Circular,
    Fsd,
    PublicApi,
    Hierarchy,
    Sdp,
    SiblingCross,
    GodFile,
    Sfc,
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
    (HealthMetric::Fsd, 2.5),
    (HealthMetric::PublicApi, 2.0),
    (HealthMetric::Hierarchy, 2.0),
    (HealthMetric::Sdp, 2.0),
    (HealthMetric::SiblingCross, 1.5),
    (HealthMetric::GodFile, 1.5),
    (HealthMetric::Sfc, 1.0),
    (HealthMetric::Diamond, 1.0),
    (HealthMetric::MainSequence, 0.5),
    (HealthMetric::Modularity, 0.5),
    (HealthMetric::Cohesion, 0.5),
    (HealthMetric::RenameInstability, 0.3),
    (HealthMetric::BusFactor, 0.3),
];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthContributor {
    pub metric: HealthMetric,
    pub weight: f64,
    /// Normalized shortfall 0-1 (how far below a perfect 100). `circular`: 1 if any cycle exists, else 0.
    pub gap: f64,
    /// `weight x gap x 10` — this metric's points of the composite.
    pub contribution: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthIndex {
    /// Composite structural debt — higher = worse. 0 = every weighted structural score is at 100 and no cycles.
    pub pain: f64,
    /// Metrics driving the number, highest contribution first — the "why" behind the scalar (zero-contributors
    /// dropped).
    pub contributors: Vec<HealthContributor>,
}

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
            HealthMetric::Fsd => gap(scores.fsd.score),
            HealthMetric::PublicApi => gap(scores.public_api.score),
            HealthMetric::Hierarchy => gap(scores.hierarchy.score),
            HealthMetric::Sdp => gap(scores.sdp.score),
            HealthMetric::SiblingCross => gap(scores.sibling_cross.score),
            HealthMetric::GodFile => gap(scores.god_file.score),
            HealthMetric::Sfc => gap(scores.sfc.score),
            HealthMetric::Diamond => gap(scores.diamond.score),
            HealthMetric::MainSequence => gap(scores.main_sequence.score),
            HealthMetric::Modularity => gap(scores.modularity.score),
            HealthMetric::Cohesion => gap(scores.cohesion.score),
            HealthMetric::RenameInstability => gap(scores.rename_instability.score),
            HealthMetric::BusFactor => gap(scores.bus_factor.score),
        }
    };

    let mut contributors: Vec<HealthContributor> = HEALTH_METRIC_WEIGHTS
        .iter()
        .map(|&(metric, weight)| {
            let g = gap_of(metric);
            HealthContributor {
                metric,
                weight,
                gap: round3(g),
                contribution: round2(weight * g * POINTS_PER_GAP),
            }
        })
        .filter(|c| c.contribution > 0.0)
        .collect();
    contributors.sort_by(|a, b| {
        b.contribution
            .partial_cmp(&a.contribution)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let pain = round1(contributors.iter().map(|c| c.contribution).sum());

    HealthIndex { pain, contributors }
}

#[cfg(test)]
mod tests {
    //! Exercises `compute_health_index`: all-perfect scores with no cycles yield pain 0 with no
    //! contributors, a cycle alone contributes the circular weight as a binary full-weight hit, and gap
    //! scales each metric's contribution with contributors sorted descending by contribution.
    use super::*;
    use crate::scores::types::{
        BusFactorScore, CohesionScore, CouplingScore, DiamondScore, FixRatioScore, FsdScore,
        GodFileScore, HierarchyScore, LodScore, MainSequenceScore, ModularityScore, PublicApiScore,
        RenameScore, SdpScore, SfcScore, SiblingCrossScore, TypeSafetyScore,
    };

    /// A `Scores` with every metric perfect (100) and no cycles -> pain 0. Callers override individual fields.
    fn perfect_scores() -> Scores {
        Scores {
            fsd: FsdScore {
                score: 100.0,
                total_imports: 0,
                violations: vec![],
            },
            cohesion: CohesionScore {
                score: 100.0,
                slices: vec![],
            },
            coupling: CouplingScore {
                score: 100.0,
                avg_fan_out: 0.0,
                max_fan_out: 0.0,
                circular_count: 0,
            },
            sdp: SdpScore {
                score: 100.0,
                total_cross_slice_edges: 0,
                violations: vec![],
            },
            hierarchy: HierarchyScore {
                score: 100.0,
                total_intra_module_edges: 0,
                violations: vec![],
            },
            public_api: PublicApiScore {
                score: 100.0,
                total_cross_module_imports: 0,
                deep_imports: vec![],
            },
            sfc: SfcScore {
                score: 100.0,
                limit: 0,
                compliant: 0,
                total: 0,
                violations: vec![],
            },
            main_sequence: MainSequenceScore {
                score: 100.0,
                avg_distance: 0.0,
                modules: vec![],
            },
            modularity: ModularityScore {
                score: 100.0,
                q: 0.0,
                edge_count: 0,
                slice_count: 0,
            },
            god_file: GodFileScore {
                score: 100.0,
                limit: 0,
                files: vec![],
            },
            sibling_cross: SiblingCrossScore {
                score: 100.0,
                total_intra_module_edges: 0,
                violations: vec![],
            },
            diamond: DiamondScore {
                score: 100.0,
                pairs: vec![],
            },
            rename_instability: RenameScore {
                score: 100.0,
                renamed: 0,
                total: 0,
                files: vec![],
            },
            bus_factor: BusFactorScore {
                score: 100.0,
                risky: 0,
                files: vec![],
            },
            fix_ratio: FixRatioScore {
                score: 100.0,
                fix: 0,
                total: 0,
                ratio: 0.0,
            },
            type_safety: TypeSafetyScore {
                score: 100.0,
                total_as_cast: 0,
                total_any_type: 0,
                violations: vec![],
            },
            lod: LodScore {
                score: 100.0,
                total_violations: 0,
                violations: vec![],
            },
        }
    }

    #[test]
    fn all_perfect_scores_no_cycle_pain_0_no_contributors() {
        let h = compute_health_index(&perfect_scores());
        assert_eq!(h.pain, 0.0);
        assert!(h.contributors.is_empty());
    }

    #[test]
    fn a_cycle_alone_contributes_circular_weight_x_10_binary_full_weight() {
        let mut scores = perfect_scores();
        scores.coupling.circular_count = 2;
        let h = compute_health_index(&scores);
        assert_eq!(h.pain, 3.0 * 10.0); // 30
        assert_eq!(h.contributors[0].metric, HealthMetric::Circular);
        assert_eq!(h.contributors[0].gap, 1.0);
    }

    #[test]
    fn gap_scales_the_contribution_and_contributors_are_sorted_by_contribution_desc() {
        // fsd at 50 -> gap 0.5 -> 2.5*0.5*10 = 12.5 ; god_file at 80 -> gap 0.2 -> 1.5*0.2*10 = 3.0
        let mut scores = perfect_scores();
        scores.fsd.score = 50.0;
        scores.god_file.score = 80.0;
        let h = compute_health_index(&scores);
        let metrics: Vec<HealthMetric> = h.contributors.iter().map(|c| c.metric).collect();
        assert_eq!(metrics, vec![HealthMetric::Fsd, HealthMetric::GodFile]);
        assert_eq!(h.contributors[0].contribution, 12.5);
        assert_eq!(h.contributors[1].contribution, 3.0);
        assert_eq!(h.pain, 15.5);
    }
}
