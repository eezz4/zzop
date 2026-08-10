//! Hierarchy — ratio of child -> ancestor (upward) imports within a module. A directory tree inside a module
//! should only import downward (parent -> child) or sideways into shared infra; a child reaching up into its own
//! ancestor's private files is a hidden coupling that makes the ancestor hard to move or refactor independently.

use super::config::ScoresConfig;
use super::detail_cap::{cap_and_count_dropped, MAX_EDGE_ROWS_LISTED};
use super::shared::{is_external, is_upward_import, module_of, round};
use super::types::{HierarchyScore, HierarchyViolation};
use zzop_core::DepGraph;

pub fn compute_hierarchy(
    dep: &DepGraph,
    cfg: &ScoresConfig,
    is_scored: &dyn Fn(&str) -> bool,
) -> HierarchyScore {
    let mut violations: Vec<HierarchyViolation> = Vec::new();
    let mut intra: u32 = 0;

    // Deterministic traversal: HashMap iteration order is unspecified, so sorting by the importer path
    // gives a stable, reproducible order.
    // Subject here is the IMPORTER (`from`), never the target — see `ScoresInput::is_scored`.
    let mut froms: Vec<&String> = dep.keys().filter(|from| is_scored(from)).collect();
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
            if is_upward_import(cfg, from, to) {
                violations.push(HierarchyViolation {
                    from: from.clone(),
                    to: to.clone(),
                    module: fm.clone(),
                });
            }
        }
    }

    violations.sort_by(|a, b| a.module.cmp(&b.module));

    let score = if intra == 0 {
        100.0
    } else {
        (100.0 - (violations.len() as f64 / intra as f64) * 100.0).max(0.0)
    };

    let violations_truncated = cap_and_count_dropped(&mut violations, MAX_EDGE_ROWS_LISTED);

    HierarchyScore {
        score: round(score),
        total_intra_module_edges: intra,
        violations,
        violations_truncated,
    }
}

#[cfg(test)]
mod tests {
    //! Covers the empty-graph baseline, a clean downward-only edge, an upward import being flagged, an
    //! index-barrel exemption for an otherwise-upward import, and cross-module/external imports being
    //! excluded from the intra-module edge count — all against `ScoresConfig::default()`.
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
        let r = compute_hierarchy(&DepGraph::new(), &cfg(), &|_| true);
        assert_eq!(r.score, 100.0);
        assert_eq!(r.total_intra_module_edges, 0);
        assert!(r.violations.is_empty());
    }

    #[test]
    fn downward_only_intra_edge_no_violation_score_100() {
        let d = dep(&[
            // parent imports child (downward) -- fine
            ("orders/a/parent.ts", &["orders/a/b/child.ts"]),
            ("orders/a/b/child.ts", &[]),
        ]);
        let r = compute_hierarchy(&d, &cfg(), &|_| true);
        assert_eq!(r.total_intra_module_edges, 1);
        assert!(r.violations.is_empty());
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn upward_import_child_to_ancestor_dir_is_a_violation() {
        let d = dep(&[("orders/a/b/child.ts", &["orders/a/parent.ts"])]);
        let r = compute_hierarchy(&d, &cfg(), &|_| true);
        assert_eq!(r.total_intra_module_edges, 1);
        assert_eq!(r.violations.len(), 1);
        assert_eq!(r.violations[0].from, "orders/a/b/child.ts");
        assert_eq!(r.violations[0].to, "orders/a/parent.ts");
        assert_eq!(r.violations[0].module, "orders");
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn upward_import_to_index_barrel_is_exempt_1_of_2_score_50() {
        let d = dep(&[
            ("orders/a/b/child.ts", &["orders/a/parent.ts"]), // upward -> violation
            ("orders/a/c/leaf.ts", &["orders/a/index.ts"]),   // upward but barrel -> exempt
        ]);
        let r = compute_hierarchy(&d, &cfg(), &|_| true);
        assert_eq!(r.total_intra_module_edges, 2);
        assert_eq!(r.violations.len(), 1);
        // 100 - (1/2)*100 = 50
        assert_eq!(r.score, 50.0);
    }

    #[test]
    fn cross_module_and_external_imports_are_not_counted() {
        let d = dep(&[(
            "orders/a/b/child.ts",
            &["billing/y.ts", "react", "orders/a/parent.ts"],
        )]);
        let r = compute_hierarchy(&d, &cfg(), &|_| true);
        // only the same-module orders edge counts
        assert_eq!(r.total_intra_module_edges, 1);
        assert_eq!(r.violations.len(), 1);
        assert_eq!(r.score, 0.0);
    }

    /// The EDGE-shaped half of the same repair (`MAX_EDGE_ROWS_LISTED`, double the per-file cap). Until
    /// 2026-07-31 this list truncated silently, and the silence bit harder here than anywhere: the score
    /// is computed from the FULL violation count BEFORE the cap, so a reader who counted the rows to
    /// check the arithmetic got a number that could not reproduce the score, with nothing saying why.
    /// `violationsTruncated` closes exactly that gap.
    #[test]
    fn a_violation_list_over_the_cap_discloses_how_many_rows_it_dropped() {
        let over = MAX_EDGE_ROWS_LISTED + 5;
        // Each child reaches UP into its own ancestor — one intra-module upward edge apiece.
        let owned: Vec<(String, Vec<String>)> = (0..over)
            .map(|i| {
                (
                    format!("orders/a/b/child{i}.ts"),
                    vec!["orders/a/parent.ts".to_string()],
                )
            })
            .collect();
        let d: DepGraph = owned.into_iter().collect();
        let r = compute_hierarchy(&d, &cfg(), &|_| true);
        assert_eq!(
            r.total_intra_module_edges, over as u32,
            "every edge counts toward the denominator, capped list or not"
        );
        assert_eq!(
            r.violations.len(),
            MAX_EDGE_ROWS_LISTED,
            "the list is capped"
        );
        assert_eq!(r.violations_truncated, 5, "and it says how many it dropped");
        assert_eq!(
            r.violations.len() + r.violations_truncated as usize,
            over,
            "list length plus the dropped count must reconstruct the count the SCORE was computed from"
        );
        // The score really was computed over all of them: every edge is a violation, so it floors at 0.
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn a_violation_list_under_the_cap_reports_zero_dropped() {
        let d = dep(&[("orders/a/b/child.ts", &["orders/a/parent.ts"])]);
        let r = compute_hierarchy(&d, &cfg(), &|_| true);
        assert_eq!(r.violations.len(), 1);
        assert_eq!(r.violations_truncated, 0);
    }
}
