//! Exercises `compute_health_index`: all-perfect scores with no cycles yield pain 0 with no
//! contributors, a cycle alone contributes the circular weight as a binary full-weight hit, and gap
//! scales each metric's contribution with contributors sorted descending by contribution.
use super::*;
use crate::scores::types::{
    BusFactorScore, CohesionScore, CouplingScore, DiamondScore, FeatureSlicedDesignScore,
    FileSizeComplianceScore, FixRatioScore, GodFileScore, HierarchyScore, MainSequenceScore,
    ModularityScore, PublicApiScore, RenameScore, SdpScore, SiblingCrossScore,
};

/// A `Scores` with every metric perfect (100) and no cycles -> pain 0. Callers override individual fields.
fn perfect_scores() -> Scores {
    Scores {
        feature_sliced_design: FeatureSlicedDesignScore {
            score: 100.0,
            total_imports: 0,
            layer_classified_imports: 0,
            violations: vec![],
        },
        cohesion: CohesionScore {
            score: 100.0,
            slice_count: 0,
            slices: vec![],
        },
        coupling: CouplingScore {
            score: 100.0,
            avg_fan_out_among_importers: 0.0,
            importer_count: 0,
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
            violations_truncated: 0,
        },
        public_api: PublicApiScore {
            score: 100.0,
            total_cross_module_imports: 0,
            deep_imports: vec![],
            deep_imports_truncated: 0,
        },
        file_size_compliance: FileSizeComplianceScore {
            score: 100.0,
            limit: 0,
            compliant: 0,
            total: 0,
            violations: vec![],
            violations_truncated: 0,
        },
        main_sequence: MainSequenceScore {
            score: 100.0,
            classified_files: 0,
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
            total: 0,
            files: vec![],
            files_truncated: 0,
        },
        sibling_cross: SiblingCrossScore {
            score: 100.0,
            total_intra_module_edges: 0,
            violations: vec![],
            violations_truncated: 0,
        },
        diamond: DiamondScore {
            score: 100.0,
            roots_examined: 0,
            pairs: vec![],
            pairs_truncated: 0,
        },
        rename_instability: RenameScore {
            score: 100.0,
            renamed: 0,
            total: 0,
            files: vec![],
            files_truncated: 0,
        },
        bus_factor: BusFactorScore {
            score: 100.0,
            total: 0,
            risky: 0,
            files: vec![],
            files_truncated: 0,
        },
        fix_ratio: FixRatioScore {
            score: 100.0,
            fix_file_touches: 0,
            tagged_file_touches: 0,
            fix_share_of_tagged_touches: 0.0,
        },
    }
}

/// A `perfect_scores()` with every population set, so every metric is MEASURED — the fixture for tests
/// about gap arithmetic, which need the renormalization factor to be exactly 1.
fn all_measured_scores() -> Scores {
    let mut s = perfect_scores();
    s.feature_sliced_design.layer_classified_imports = 40;
    s.cohesion.slice_count = 3;
    s.coupling.importer_count = 20;
    s.sdp.total_cross_slice_edges = 12;
    s.hierarchy.total_intra_module_edges = 30;
    s.public_api.total_cross_module_imports = 18;
    s.file_size_compliance.total = 50;
    s.main_sequence.classified_files = 50;
    s.modularity.edge_count = 60;
    s.god_file.total = 50;
    s.sibling_cross.total_intra_module_edges = 30;
    s.diamond.roots_examined = 50;
    s.rename_instability.total = 50;
    s.bus_factor.total = 9;
    s
}

#[test]
fn all_perfect_scores_no_cycle_pain_0_no_contributors() {
    let h = compute_health_index(&all_measured_scores());
    assert_eq!(h.pain, Some(0.0));
    assert!(
        h.contributors.is_empty(),
        "every metric was measured and clean, so nothing explains the score and nothing is dark"
    );
}

/// RED (defect a): a metric that scored 100 because it had NOTHING TO MEASURE must not be
/// byte-identical to one that scored 100 because the code is clean.
///
/// `perfect_scores()` above is the empty-population case in its purest form — every denominator is 0,
/// so not one of the fourteen weighted metrics judged a single subject. Before this pin it returned
/// `pain: 0.0` with an EMPTY `contributors` list: the same two values a genuinely spotless tree gets.
/// A reader holding that reply could not tell "nothing is wrong" from "nothing was looked at".
#[test]
fn an_unmeasured_metric_is_distinguishable_from_a_clean_one() {
    let nothing_measured = compute_health_index(&perfect_scores());

    let mut clean = perfect_scores();
    // Same perfect scores, but every metric actually judged a population.
    clean.feature_sliced_design.layer_classified_imports = 40;
    clean.cohesion.slice_count = 3;
    clean.coupling.importer_count = 20;
    clean.sdp.total_cross_slice_edges = 12;
    clean.hierarchy.total_intra_module_edges = 30;
    clean.public_api.total_cross_module_imports = 18;
    clean.file_size_compliance.total = 50;
    clean.file_size_compliance.compliant = 50;
    clean.main_sequence.classified_files = 50;
    clean.modularity.edge_count = 60;
    clean.god_file.total = 50;
    clean.sibling_cross.total_intra_module_edges = 30;
    clean.diamond.roots_examined = 50;
    clean.rename_instability.total = 50;
    clean.bus_factor.total = 9;
    let clean = compute_health_index(&clean);

    assert_ne!(
        nothing_measured, clean,
        "a tree where NO metric had a population must not produce the same HealthIndex as a tree \
         where every metric judged a real population and found nothing wrong — an unmeasurable axis \
         made a repo look healthier, which is the defect this pin exists for"
    );
    assert_eq!(
        nothing_measured.pain, None,
        "with zero measured weight there is no population to renormalize over, so `pain` is absence \
         of data, not 0.0 — the same `null` vs `[]` convention `co_change` already uses"
    );
    assert_eq!(
        clean.pain,
        Some(0.0),
        "a genuinely clean tree still scores 0 pain"
    );
}

/// RED (defect a, second face): an unmeasurable metric must not lower `pain`, and the fourteen
/// weights must renormalize over the MEASURED subset only.
///
/// Before this, `featureSlicedDesign` scoring 100 on a tree that never adopted the convention removed
/// its 2.5 weight worth of possible pain from the total and left every other metric's contribution
/// untouched — so the repo read strictly healthier than one where the axis was measured and passed.
#[test]
fn an_unmeasured_metric_does_not_dilute_pain_it_leaves_the_weighting() {
    /// `godFile` at 80 -> gap 0.2, weight 1.5 -> 3.0 raw points.
    fn with_god_file_at_80() -> Scores {
        let mut s = perfect_scores();
        s.god_file.score = 80.0;
        s.god_file.total = 50;
        s
    }

    // Case 1: every metric measured; only godFile is imperfect.
    let mut all_measured = with_god_file_at_80();
    all_measured.feature_sliced_design.layer_classified_imports = 40;
    all_measured.cohesion.slice_count = 3;
    all_measured.coupling.importer_count = 20;
    all_measured.sdp.total_cross_slice_edges = 12;
    all_measured.hierarchy.total_intra_module_edges = 30;
    all_measured.public_api.total_cross_module_imports = 18;
    all_measured.file_size_compliance.total = 50;
    all_measured.main_sequence.classified_files = 50;
    all_measured.modularity.edge_count = 60;
    all_measured.sibling_cross.total_intra_module_edges = 30;
    all_measured.diamond.roots_examined = 50;
    all_measured.rename_instability.total = 50;
    all_measured.bus_factor.total = 9;
    let all_measured = compute_health_index(&all_measured);

    // Case 2: identical, except featureSlicedDesign had no population at all (a tree that never
    // adopted the convention). Its 2.5 weight must LEAVE the denominator, not sit in it scoring 100.
    let mut fsd_dark = with_god_file_at_80();
    fsd_dark.feature_sliced_design.layer_classified_imports = 0;
    fsd_dark.cohesion.slice_count = 3;
    fsd_dark.coupling.importer_count = 20;
    fsd_dark.sdp.total_cross_slice_edges = 12;
    fsd_dark.hierarchy.total_intra_module_edges = 30;
    fsd_dark.public_api.total_cross_module_imports = 18;
    fsd_dark.file_size_compliance.total = 50;
    fsd_dark.main_sequence.classified_files = 50;
    fsd_dark.modularity.edge_count = 60;
    fsd_dark.sibling_cross.total_intra_module_edges = 30;
    fsd_dark.diamond.roots_examined = 50;
    fsd_dark.rename_instability.total = 50;
    fsd_dark.bus_factor.total = 9;
    let fsd_dark = compute_health_index(&fsd_dark);

    assert!(
        fsd_dark.pain.expect("measured") > all_measured.pain.expect("measured"),
        "losing an axis must never make the repo look HEALTHIER: pain over a smaller measured \
         population is renormalized UP, because the same 3.0 raw points are now the whole story \
         rather than one metric among fourteen. Got dark={:?} measured={:?}",
        fsd_dark.pain,
        all_measured.pain
    );
    assert_eq!(
        all_measured.measured_weight,
        total_health_weight(),
        "every metric had a population, so the measured weight is the full table"
    );
    assert_eq!(
        fsd_dark.measured_weight,
        total_health_weight() - 2.5,
        "featureSlicedDesign's weight must leave the denominator entirely"
    );
    // The renormalization is exact: same raw points, scaled by total/measured.
    assert_eq!(
        fsd_dark.pain,
        Some(round1(
            3.0 * total_health_weight() / (total_health_weight() - 2.5)
        ))
    );
}

/// RED (defect a, third face): an unmeasured metric must be VISIBLE, not dropped.
///
/// `contributors` filtered on `contribution > 0.0`, so a zero-population metric vanished from the
/// reply entirely — the reader was never told the axis existed, let alone that it was dark. It now
/// rides the list with `population: 0` and a `null` gap/contribution, which is the shape the rest of
/// this repo already uses for "never measured" (see `zzop_facade::query_coverage`'s three-value rule).
#[test]
fn a_zero_population_metric_rides_contributors_with_a_null_gap() {
    let mut scores = perfect_scores();
    // Only godFile measured anything; every other axis is dark. It also has a real gap, so it earns a
    // place in the ranked "why" — a measured-and-CLEAN metric explains nothing and is dropped, which is
    // the distinction this test's second half checks.
    scores.god_file.total = 50;
    scores.god_file.score = 80.0;

    let h = compute_health_index(&scores);
    let fsd = h
        .contributors
        .iter()
        .find(|c| c.metric == HealthMetric::FeatureSlicedDesign)
        .expect(
            "an unmeasured metric must still appear in `contributors` — dropping it is what made an \
             unmeasurable axis indistinguishable from a clean one",
        );
    assert_eq!(fsd.population, 0);
    assert_eq!(fsd.gap, None, "no population means no gap to report");
    assert_eq!(fsd.contribution, None);

    let god = h
        .contributors
        .iter()
        .find(|c| c.metric == HealthMetric::GodFile)
        .expect("the one measured metric is present");
    assert_eq!(god.population, 50);
    assert_eq!(god.gap, Some(0.2));
    assert_eq!(god.contribution, Some(3.0));

    // A measured-and-CLEAN metric is `Some(0.0)`, never `None` — the two must stay tellable apart even
    // though the clean one is dropped from the ranked list.
    let mut clean_measured = perfect_scores();
    clean_measured.god_file.total = 50;
    let h = compute_health_index(&clean_measured);
    assert!(
        !h.contributors
            .iter()
            .any(|c| c.metric == HealthMetric::GodFile),
        "measured-and-clean explains nothing, so it stays out of the ranked why"
    );
    assert_eq!(
        h.pain,
        Some(0.0),
        "one metric measured and clean -> 0 pain over a measured weight of 1.5, not `null`"
    );
    assert_eq!(h.measured_weight, 1.5);
}

#[test]
fn a_cycle_alone_contributes_circular_weight_x_10_binary_full_weight() {
    let mut scores = all_measured_scores();
    scores.coupling.circular_count = 2;
    let h = compute_health_index(&scores);
    assert_eq!(h.pain, Some(3.0 * 10.0)); // 30
    assert_eq!(h.contributors[0].metric, HealthMetric::Circular);
    assert_eq!(h.contributors[0].gap, Some(1.0));
}

/// `circular`'s population is `coupling.importerCount` — a tree where nothing imports anything cannot
/// have been searched for cycles, so "no cycles" is not a finding there.
#[test]
fn circular_is_unmeasured_when_no_file_imports_anything() {
    let mut scores = all_measured_scores();
    scores.coupling.importer_count = 0;
    let h = compute_health_index(&scores);
    let circular = h
        .contributors
        .iter()
        .find(|c| c.metric == HealthMetric::Circular)
        .expect("an unmeasured metric still rides contributors");
    assert_eq!(circular.population, 0);
    assert_eq!(circular.gap, None);
    assert_eq!(
        h.measured_weight,
        // `circular` (3.0) and `coupling` are two metrics but ONE population — coupling is not in the
        // weight table at all, so only circular's weight leaves.
        total_health_weight() - 3.0
    );
}

#[test]
fn gap_scales_the_contribution_and_contributors_are_sorted_by_contribution_desc() {
    // feature_sliced_design at 50 -> gap 0.5 -> 2.5*0.5*10 = 12.5 ; god_file at 80 -> gap 0.2 -> 1.5*0.2*10 = 3.0
    let mut scores = all_measured_scores();
    scores.feature_sliced_design.score = 50.0;
    scores.god_file.score = 80.0;
    let h = compute_health_index(&scores);
    let metrics: Vec<HealthMetric> = h.contributors.iter().map(|c| c.metric).collect();
    assert_eq!(
        metrics,
        vec![HealthMetric::FeatureSlicedDesign, HealthMetric::GodFile]
    );
    assert_eq!(h.contributors[0].contribution, Some(12.5));
    assert_eq!(h.contributors[1].contribution, Some(3.0));
    // Every metric measured -> the renormalization factor is exactly 1, so pain is the raw sum.
    assert_eq!(h.measured_weight, total_health_weight());
    assert_eq!(h.pain, Some(15.5));
}
