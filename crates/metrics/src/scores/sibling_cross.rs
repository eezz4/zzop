//! Sibling Cross — ratio of horizontal imports between sub-directories within the same module (e.g. `ui/` reaching
//! into `data/`). Unlike hierarchy (parent/child), this catches lateral coupling between peer sub-directories that
//! should talk through a shared/public surface rather than directly.

use super::config::ScoresConfig;
use super::shared::{is_external, module_of, round, top_subdir};
use super::types::{SiblingCross, SiblingCrossScore};
use zzop_core::DepGraph;

/// Caps the returned violation list (not the score).
const MAX_VIOLATIONS_LISTED: usize = 100;

pub fn compute_sibling_cross(dep: &DepGraph, cfg: &ScoresConfig) -> SiblingCrossScore {
    let mut violations: Vec<SiblingCross> = Vec::new();
    let mut intra: u32 = 0;

    // Deterministic traversal: HashMap iteration order is unspecified, so sorting by the importer path
    // gives a stable, reproducible order.
    let mut froms: Vec<&String> = dep.keys().collect();
    froms.sort();

    for from in froms {
        let fm = match module_of(cfg, from) {
            Some(m) => m,
            None => continue,
        };
        for to in &dep[from] {
            if is_external(to) {
                continue;
            }
            if module_of(cfg, to).as_deref() != Some(fm.as_str()) {
                continue;
            }
            intra += 1;
            let from_subdir = top_subdir(from, &fm);
            let to_subdir = top_subdir(to, &fm);
            let (from_subdir, to_subdir) = match (from_subdir, to_subdir) {
                (Some(f), Some(t)) => (f, t),
                _ => continue,
            };
            if from_subdir == to_subdir {
                continue;
            }
            if cfg.hierarchy_shared_dirs.contains(&from_subdir)
                || cfg.hierarchy_shared_dirs.contains(&to_subdir)
            {
                continue;
            }
            violations.push(SiblingCross {
                from: from.clone(),
                to: to.clone(),
                module: fm.clone(),
                from_subdir,
                to_subdir,
            });
        }
    }

    violations.sort_by(|a, b| a.module.cmp(&b.module));

    let score = if intra == 0 {
        100.0
    } else {
        (100.0 - (violations.len() as f64 / intra as f64) * 100.0).max(0.0)
    };

    violations.truncate(MAX_VIOLATIONS_LISTED);

    SiblingCrossScore {
        score: round(score),
        total_intra_module_edges: intra,
        violations,
    }
}

#[cfg(test)]
mod tests {
    //! Covers the empty-graph baseline, a same-subdir edge being exempt, a cross-sibling import being
    //! flagged, an import into a shared subdir being exempt, and a mixed graph with two violations among
    //! five intra-module edges — all against `ScoresConfig::default()`.
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
    fn empty_graph_score_100() {
        let r = compute_sibling_cross(&DepGraph::new(), &cfg());
        assert_eq!(r.score, 100.0);
        assert_eq!(r.total_intra_module_edges, 0);
        assert!(r.violations.is_empty());
    }

    #[test]
    fn same_subdir_intra_edge_no_violation_score_100() {
        let d = dep(&[
            ("orders/ui/Btn.ts", &["orders/ui/Card.ts"]),
            ("orders/ui/Card.ts", &[]),
        ]);
        let r = compute_sibling_cross(&d, &cfg());
        assert_eq!(r.total_intra_module_edges, 1);
        assert!(r.violations.is_empty());
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn cross_sibling_import_ui_to_data_is_a_violation() {
        let d = dep(&[("orders/ui/Btn.ts", &["orders/data/api.ts"])]);
        let r = compute_sibling_cross(&d, &cfg());
        assert_eq!(r.total_intra_module_edges, 1);
        assert_eq!(r.violations.len(), 1);
        assert_eq!(r.violations[0].from, "orders/ui/Btn.ts");
        assert_eq!(r.violations[0].to, "orders/data/api.ts");
        assert_eq!(r.violations[0].module, "orders");
        assert_eq!(r.violations[0].from_subdir, "ui");
        assert_eq!(r.violations[0].to_subdir, "data");
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn import_into_a_shared_subdir_utils_is_exempt_score_100() {
        let d = dep(&[("orders/ui/Btn.ts", &["orders/utils/h.ts"])]);
        let r = compute_sibling_cross(&d, &cfg());
        assert_eq!(r.total_intra_module_edges, 1);
        assert!(r.violations.is_empty());
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn mixed_graph_two_violations_of_five_intra_edges_score_60() {
        let d = dep(&[
            (
                "orders/ui/Btn.ts",
                &[
                    "orders/data/api.ts", // ui -> data -> violation
                    "orders/utils/h.ts",  // toSubdir shared -> exempt
                    "orders/ui/Card.ts",  // same subdir -> not a violation
                ],
            ),
            ("orders/data/api.ts", &["orders/ui/Btn.ts"]), // data -> ui -> violation
            ("orders/index.ts", &["orders/ui/Btn.ts"]),    // fromSubdir is null (index.ts) -> skip
        ]);
        let r = compute_sibling_cross(&d, &cfg());
        assert_eq!(r.total_intra_module_edges, 5);
        assert_eq!(r.violations.len(), 2);
        // 100 - (2/5)*100 = 60
        assert_eq!(r.score, 60.0);
    }
}
