//! Single File Component — ratio of files within the per-target LOC limit. A file over the limit is not
//! automatically wrong, but it correlates with too many responsibilities living in one place; the score
//! rewards a codebase that mostly stays under its role's convention.
//!
//! Non-source files (data/config/assets with real `loc` but no parser frontend — a `pnpm-lock.yaml`, a
//! `.css`, a huge generated `.json`) are excluded from BOTH the `violations` list AND the
//! `total`/`compliant` denominator, not merely never-flagged: their size is not a single-responsibility
//! signal, so counting them as compliant "live" files would silently inflate the compliant ratio too.
//! Source-ness is injected as an `is_source` closure (mirroring `zzop_core::build_file_nodes`) — this
//! module never hardcodes language/extension knowledge; the engine's dispatch table is the single source
//! of truth, passed in.

use super::config::ScoresConfig;
use super::types::{SfcScore, SfcViolation};
use zzop_core::FileNode;

/// Detail list cap.
const MAX_DETAIL_ITEMS: usize = 50;

pub fn compute_sfc<F>(
    nodes: &[FileNode],
    target: Option<&str>,
    cfg: &ScoresConfig,
    is_source: F,
) -> SfcScore
where
    F: Fn(&str) -> bool,
{
    let limit = cfg.thresholds.loc_limit(target);
    let live: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| n.loc > 0 && is_source(&n.id))
        .collect();
    let mut violations: Vec<SfcViolation> = Vec::new();
    let mut compliant = 0u32;
    for n in &live {
        if n.loc <= limit {
            compliant += 1;
        } else {
            violations.push(SfcViolation {
                path: n.path.clone(),
                loc: n.loc,
                limit,
            });
        }
    }
    violations.sort_by_key(|v| std::cmp::Reverse(v.loc));
    let score = if live.is_empty() {
        100.0
    } else {
        (compliant as f64 / live.len() as f64) * 100.0
    };
    violations.truncate(MAX_DETAIL_ITEMS);
    SfcScore {
        score: score.round(),
        limit,
        compliant,
        total: live.len() as u32,
        violations,
    }
}

#[cfg(test)]
mod tests {
    //! Covers the no-live-files baseline, the LOC-equals-limit boundary counting as compliant, a
    //! half-compliant case, violations sorted by LOC descending, and the per-target limit override —
    //! all against `ScoresConfig::default()`.
    use super::*;

    fn node(path: &str, loc: u32) -> FileNode {
        FileNode {
            id: path.to_string(),
            path: path.to_string(),
            change_count: 0,
            churn: 0,
            last_modified: None,
            author_count: 1,
            loc,
            tag_counts: Default::default(),
            fan_in: 0,
            fan_out: 0,
            total_connections: 0,
            risk_score: 0.0,
            ..Default::default()
        }
    }

    #[test]
    fn no_live_files_score_100_default_limit_150() {
        let r = compute_sfc(&[], None, &ScoresConfig::default(), |_| true);
        assert_eq!(r.score, 100.0);
        assert_eq!(r.limit, 150);
        assert_eq!(r.compliant, 0);
        assert_eq!(r.total, 0);
        assert_eq!(r.violations, vec![]);
    }

    #[test]
    fn all_files_within_limit_boundary_loc_eq_limit_is_compliant_score_100() {
        // default limit 150; loc 150 is <= limit -> compliant
        let nodes = [node("a", 150), node("b", 50)];
        let r = compute_sfc(&nodes, None, &ScoresConfig::default(), |_| true);
        assert_eq!(r.score, 100.0);
        assert_eq!(r.compliant, 2);
        assert_eq!(r.total, 2);
        assert_eq!(r.violations, vec![]);
    }

    #[test]
    fn half_compliant_score_50() {
        // compliant=1, total=2 -> (1/2)*100 = 50
        let nodes = [node("ok", 100), node("big", 300)];
        let r = compute_sfc(&nodes, None, &ScoresConfig::default(), |_| true);
        assert_eq!(r.score, 50.0);
        assert_eq!(r.compliant, 1);
        assert_eq!(r.total, 2);
        assert_eq!(
            r.violations,
            vec![SfcViolation {
                path: "big".to_string(),
                loc: 300,
                limit: 150
            }]
        );
    }

    #[test]
    fn violations_sorted_by_loc_desc() {
        let nodes = [node("med", 200), node("huge", 500), node("ok", 50)];
        let r = compute_sfc(&nodes, None, &ScoresConfig::default(), |_| true);
        // compliant=1, total=3 -> round((1/3)*100) = 33
        assert_eq!(r.score, 33.0);
        let paths: Vec<&str> = r.violations.iter().map(|v| v.path.as_str()).collect();
        assert_eq!(paths, vec!["huge", "med"]);
    }

    #[test]
    fn target_be_raises_the_limit_to_200() {
        // target be -> limit 200; loc 180 compliant, loc 250 violation
        let nodes = [node("a", 180), node("b", 250)];
        let r = compute_sfc(&nodes, Some("be"), &ScoresConfig::default(), |_| true);
        assert_eq!(r.limit, 200);
        assert_eq!(r.compliant, 1);
        assert_eq!(r.score, 50.0);
        assert_eq!(
            r.violations,
            vec![SfcViolation {
                path: "b".to_string(),
                loc: 250,
                limit: 200
            }]
        );
    }

    #[test]
    fn non_source_huge_file_excluded_from_totals_and_violations() {
        // A huge non-source file (e.g. a lockfile) must not count toward `total`/`compliant` NOR appear in
        // `violations` — treated as absent from the metric entirely, so a lone compliant source file still
        // scores a perfect 100.
        let nodes = [node("pnpm-lock.yaml", 5174), node("src/app.ts", 100)];
        let r = compute_sfc(&nodes, None, &ScoresConfig::default(), |id| {
            id == "src/app.ts"
        });
        assert_eq!(r.total, 1);
        assert_eq!(r.compliant, 1);
        assert_eq!(r.violations, vec![]);
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn same_huge_file_classified_as_source_still_produces_a_violation() {
        // Same 5174 LOC, but source: the gate is about source-ness, not the limit, so it still violates.
        let nodes = [node("src/huge.ts", 5174)];
        let r = compute_sfc(&nodes, None, &ScoresConfig::default(), |_| true);
        assert_eq!(r.total, 1);
        assert_eq!(r.compliant, 0);
        assert_eq!(
            r.violations,
            vec![SfcViolation {
                path: "src/huge.ts".to_string(),
                loc: 5174,
                limit: 150
            }]
        );
    }
}
