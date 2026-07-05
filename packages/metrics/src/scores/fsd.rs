//! FSD (Feature-Sliced Design) score — ratio of layer-reverse and same-layer cross-slice imports. A lower layer
//! reaching into a higher one ("layer-reverse") or one L2 slice reaching directly into another L2 slice
//! ("cross-slice") both violate FSD's dependency-direction rule; this metric turns that violation count into a
//! 0-100 score.

use super::config::ScoresConfig;
use super::shared::{classify_path, is_external, round};
use super::types::{FsdScore, FsdViolation, FsdViolationKind};
use zpz_core::DepGraph;

/// FSD L2 (slice) layer number.
const SLICE_LAYER: u8 = 2;

pub fn compute_fsd(dep: &DepGraph, cfg: &ScoresConfig) -> FsdScore {
    let mut violations: Vec<FsdViolation> = Vec::new();
    let mut total: u32 = 0;

    // Deterministic traversal: HashMap iteration order is unspecified, so sorting by the importer path
    // gives a stable, reproducible violation order.
    let mut froms: Vec<&String> = dep.keys().collect();
    froms.sort();

    for from in froms {
        let from_info = classify_path(cfg, from);
        for to in &dep[from] {
            if is_external(to) {
                continue;
            }
            total += 1;
            let to_info = classify_path(cfg, to);
            if to_info.layer < from_info.layer {
                violations.push(FsdViolation {
                    from: from.clone(),
                    to: to.clone(),
                    kind: FsdViolationKind::LayerReverse,
                    from_layer: from_info.layer,
                    to_layer: to_info.layer,
                    from_slice: from_info.slice.clone(),
                    to_slice: to_info.slice.clone(),
                });
            } else if from_info.layer == SLICE_LAYER
                && to_info.layer == SLICE_LAYER
                && from_info.slice != to_info.slice
            {
                violations.push(FsdViolation {
                    from: from.clone(),
                    to: to.clone(),
                    kind: FsdViolationKind::CrossSlice,
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

    FsdScore {
        score: round(score),
        total_imports: total,
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
        let r = compute_fsd(&DepGraph::new(), &cfg());
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
        let r = compute_fsd(&d, &cfg());
        assert_eq!(r.total_imports, 2);
        assert!(r.violations.is_empty());
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn layer_reverse_import_l2_to_l1_is_a_violation() {
        let d = dep(&[("features/auth/login.ts", &["pages/home.ts"])]);
        let r = compute_fsd(&d, &cfg());
        assert_eq!(r.total_imports, 1);
        assert_eq!(r.violations.len(), 1);
        assert_eq!(r.violations[0].kind, FsdViolationKind::LayerReverse);
        assert_eq!(r.violations[0].from_layer, 2);
        assert_eq!(r.violations[0].to_layer, 1);
        // 100 - (1/1)*100 = 0
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn same_layer_cross_slice_l2_to_l2_different_slice_is_a_violation() {
        let d = dep(&[("features/auth/login.ts", &["features/cart/cart.ts"])]);
        let r = compute_fsd(&d, &cfg());
        assert_eq!(r.violations.len(), 1);
        assert_eq!(r.violations[0].kind, FsdViolationKind::CrossSlice);
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
        let r = compute_fsd(&d, &cfg());
        assert_eq!(r.total_imports, 4);
        assert_eq!(r.violations.len(), 2);
        // 100 - (2/4)*100 = 50
        assert_eq!(r.score, 50.0);
    }
}
