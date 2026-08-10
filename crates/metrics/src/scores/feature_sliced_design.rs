//! FSD (Feature-Sliced Design) score — ratio of layer-reverse and same-layer cross-slice imports. A lower layer
//! reaching into a higher one ("layer-reverse") or one L2 slice reaching directly into another L2 slice
//! ("cross-slice") both violate FSD's dependency-direction rule; this metric turns that violation count into a
//! 0-100 score.

use super::config::ScoresConfig;
use super::shared::{classify_path, is_external, round};
use super::types::{
    FeatureSlicedDesignScore, FeatureSlicedDesignViolation, FeatureSlicedDesignViolationKind,
};
use zzop_core::DepGraph;

/// FSD L2 (slice) layer number.
const SLICE_LAYER: u8 = 2;

/// The layer `classify_path` assigns when NO declared layer name matched — "base/external", and also
/// what every path in a tree that declares no FSD vocabulary gets. An edge with this on both ends is
/// evidence of nothing, which is why it does not count toward the metric's population.
const UNCLASSIFIED_LAYER: u8 = 4;

pub fn compute_feature_sliced_design(
    dep: &DepGraph,
    cfg: &ScoresConfig,
    is_scored: &dyn Fn(&str) -> bool,
) -> FeatureSlicedDesignScore {
    let mut violations: Vec<FeatureSlicedDesignViolation> = Vec::new();
    let mut total: u32 = 0;
    // POPULATION — see `FeatureSlicedDesignScore::layer_classified_imports`. `total` is the RATIO
    // denominator and is not the same question: it counts every in-tree import, so a tree that never
    // adopted FSD still produces a large `total`, zero violations, and a perfect score. This counts only
    // imports where at least one endpoint landed in a DECLARED layer, which is the evidence that the
    // convention is in use at all.
    let mut layer_classified: u32 = 0;

    // Deterministic traversal: HashMap iteration order is unspecified, so sorting by the importer path
    // gives a stable, reproducible violation order.
    // Subject here is the IMPORTER (`from`), never the target — see `ScoresInput::is_scored`.
    let mut froms: Vec<&String> = dep.keys().filter(|from| is_scored(from)).collect();
    froms.sort();

    for from in froms {
        let from_info = classify_path(cfg, from);
        for to in &dep[from] {
            if is_external(to) {
                continue;
            }
            total += 1;
            let to_info = classify_path(cfg, to);
            // `UNCLASSIFIED_LAYER` is the catch-all both endpoints fall into when no declared layer
            // name matches, so "either side is below it" == "this edge is evidence of the convention".
            if from_info.layer < UNCLASSIFIED_LAYER || to_info.layer < UNCLASSIFIED_LAYER {
                layer_classified += 1;
            }
            if to_info.layer < from_info.layer {
                violations.push(FeatureSlicedDesignViolation {
                    from: from.clone(),
                    to: to.clone(),
                    kind: FeatureSlicedDesignViolationKind::LayerReverse,
                    from_layer: from_info.layer,
                    to_layer: to_info.layer,
                    from_slice: from_info.slice.clone(),
                    to_slice: to_info.slice.clone(),
                });
            } else if from_info.layer == SLICE_LAYER
                && to_info.layer == SLICE_LAYER
                && from_info.slice != to_info.slice
            {
                violations.push(FeatureSlicedDesignViolation {
                    from: from.clone(),
                    to: to.clone(),
                    kind: FeatureSlicedDesignViolationKind::CrossSlice,
                    from_layer: from_info.layer,
                    to_layer: to_info.layer,
                    from_slice: from_info.slice.clone(),
                    to_slice: to_info.slice.clone(),
                });
            }
        }
    }

    let score = if total == 0 {
        100.0
    } else {
        (100.0 - (violations.len() as f64 / total as f64) * 100.0).max(0.0)
    };

    FeatureSlicedDesignScore {
        score: round(score),
        total_imports: total,
        layer_classified_imports: layer_classified,
        violations,
    }
}

#[cfg(test)]
mod tests {
    //! Covers the empty-graph baseline, clean downward/same-slice imports, layer-reverse and cross-slice
    //! violations individually, and a mixed case with both violation kinds among four imports.
    use super::*;

    fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
        pairs
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    fn cfg() -> ScoresConfig {
        ScoresConfig::default()
    }

    #[test]
    fn empty_graph_score_100_no_violations() {
        let r = compute_feature_sliced_design(&DepGraph::new(), &cfg(), &|_| true);
        assert_eq!(r.score, 100.0);
        assert_eq!(r.total_imports, 0);
        assert!(r.violations.is_empty());
    }

    #[test]
    fn clean_downward_and_same_slice_imports_score_100() {
        let d = dep(&[
            ("pages/home.ts", &["features/auth/login.ts"]),
            ("features/auth/login.ts", &["utils/x.ts"]),
            ("utils/x.ts", &[]),
        ]);
        let r = compute_feature_sliced_design(&d, &cfg(), &|_| true);
        assert_eq!(r.total_imports, 2);
        assert!(r.violations.is_empty());
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn layer_reverse_import_l2_to_l1_is_a_violation() {
        let d = dep(&[("features/auth/login.ts", &["pages/home.ts"])]);
        let r = compute_feature_sliced_design(&d, &cfg(), &|_| true);
        assert_eq!(r.total_imports, 1);
        assert_eq!(r.violations.len(), 1);
        assert_eq!(
            r.violations[0].kind,
            FeatureSlicedDesignViolationKind::LayerReverse
        );
        assert_eq!(r.violations[0].from_layer, 2);
        assert_eq!(r.violations[0].to_layer, 1);
        // 100 - (1/1)*100 = 0
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn same_layer_cross_slice_l2_to_l2_different_slice_is_a_violation() {
        let d = dep(&[("features/auth/login.ts", &["features/cart/cart.ts"])]);
        let r = compute_feature_sliced_design(&d, &cfg(), &|_| true);
        assert_eq!(r.violations.len(), 1);
        assert_eq!(
            r.violations[0].kind,
            FeatureSlicedDesignViolationKind::CrossSlice
        );
        assert_eq!(
            r.violations[0].from_slice,
            Some("features/auth".to_string())
        );
        assert_eq!(r.violations[0].to_slice, Some("features/cart".to_string()));
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn mixed_two_violations_of_four_imports_score_50() {
        let d = dep(&[
            ("pages/home.ts", &["features/auth/login.ts"]), // ok down
            (
                "features/auth/login.ts",
                &[
                    "pages/home.ts",         // layer-reverse
                    "features/cart/cart.ts", // cross-slice
                    "utils/x.ts",            // ok down (L2 -> L3)
                    "react",                 // external, skipped
                ],
            ),
        ]);
        let r = compute_feature_sliced_design(&d, &cfg(), &|_| true);
        assert_eq!(r.total_imports, 4);
        assert_eq!(r.violations.len(), 2);
        // 100 - (2/4)*100 = 50
        assert_eq!(r.score, 50.0);
    }
}

#[cfg(test)]
mod population_tests {
    //! RED (defect c): a tree that never adopted Feature-Sliced Design must not be SCORED against it.
    use super::*;

    fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
        pairs
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    /// A Rust/Go-shaped tree: real imports, none of them in any declared FSD layer. The score is a
    /// perfect 100 and always was — no path can violate a layer rule when no path has a layer — so the
    /// number is not wrong, it is UNFOUNDED. `total_imports` cannot say so (it is large and healthy
    /// looking); `layer_classified_imports` is the field that does, and it is what keeps FSD's weight of
    /// 2.5 — the second highest in the composite — out of `health.pain` for a repo it cannot judge.
    #[test]
    fn a_tree_with_no_fsd_layers_reports_a_zero_population_despite_a_perfect_score() {
        let d = dep(&[
            ("internal/service/user.go", &["internal/store/store.go"]),
            ("internal/store/store.go", &["pkg/util/str.go"]),
            ("cmd/server/main.go", &["internal/service/user.go"]),
            ("pkg/util/str.go", &[]),
        ]);
        let r = compute_feature_sliced_design(&d, &ScoresConfig::default(), &|_| true);

        assert_eq!(
            r.score, 100.0,
            "nothing can violate a rule that applies to nothing"
        );
        assert_eq!(
            r.total_imports, 3,
            "the ratio denominator still counts every import"
        );
        assert_eq!(
            r.layer_classified_imports, 0,
            "NOT ONE import touched a declared FSD layer, so this metric measured nothing — the 100 \
             above describes the absence of a convention, not adherence to one"
        );
    }

    /// The converse, so the population cannot be trivially zero: a real FSD tree reports a real one.
    #[test]
    fn a_tree_that_does_use_fsd_layers_reports_a_real_population() {
        let d = dep(&[
            ("pages/home.ts", &["features/auth/login.ts"]),
            ("features/auth/login.ts", &["shared/ui/button.ts"]),
        ]);
        let r = compute_feature_sliced_design(&d, &ScoresConfig::default(), &|_| true);
        assert_eq!(r.total_imports, 2);
        assert_eq!(r.layer_classified_imports, 2);
    }

    /// The Go/gRPC-Gateway collision, reproduced. `api/` is where protobuf output lands — the BOTTOM of
    /// that stack — while the starter config's `vocabulary.featureSlicedDesign.entry` lists `api` as a
    /// TOP entry-layer name, so every `internal/... -> api/...` import reads as a layer reversal.
    ///
    /// The population does NOT suppress this, and that is the honest outcome rather than a shortfall:
    /// the tree really does have imports FSD classified, so the metric is measured, not dark. What the
    /// field buys is legibility — the two violations sit against a population of 2, not against the
    /// tree's 4 imports, so a reader can see the verdict rests on the two edges that touch `api/`. The
    /// actual fix for such a tree is its own vocabulary (drop `api` from `entry`, or declare the FSD
    /// block empty to switch the axis off entirely).
    #[test]
    fn the_go_api_directory_collides_with_the_entry_layer_name_and_the_population_shows_how_narrowly(
    ) {
        let d = dep(&[
            (
                "internal/service/user.go",
                &["api/v1/user.pb.go", "internal/store/store.go"],
            ),
            ("internal/service/order.go", &["api/v1/order.pb.go"]),
            ("internal/store/store.go", &["pkg/util/str.go"]),
        ]);
        let r = compute_feature_sliced_design(&d, &ScoresConfig::default(), &|_| true);

        assert_eq!(r.total_imports, 4);
        assert_eq!(
            r.violations.len(),
            2,
            "both are api/ name collisions, not real layer reversals"
        );
        assert_eq!(
            r.layer_classified_imports, 2,
            "exactly the two api/-touching edges are FSD-classified — the verdict rests on those, and \
             the population is what makes that visible instead of implying a 2-of-4 failure rate"
        );
        // Every violation names `api/` on its target side: none of them is about the Go code's own layering.
        assert!(r.violations.iter().all(|v| v.to.starts_with("api/")));
    }
}
