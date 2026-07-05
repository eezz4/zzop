//! FIX commit ratio — proportion of `[FIX]`-tagged commits out of all commits. Lower means fewer reactive fixes
//! (bugs found after the fact) relative to total churn.

use crate::scores::config::ScoresConfig;
use crate::scores::types::FixRatioScore;
use zpz_core::FileNode;

/// The 0-100 score scale.
const PERCENT: f64 = 100.0;

/// `ratio = fix / total` across all tag counts on all nodes; `score = clamp(round((1 - ratio/cap) * 100), 0, 100)`.
/// `ratio` is 0 when there are no tagged commits at all.
pub fn compute_fix_ratio(nodes: &[FileNode], cfg: &ScoresConfig) -> FixRatioScore {
    let mut fix: u32 = 0;
    let mut total: u32 = 0;
    for n in nodes {
        for (tag, count) in &n.tag_counts {
            total += count;
            if tag == "FIX" {
                fix += count;
            }
        }
    }

    let ratio = if total > 0 {
        f64::from(fix) / f64::from(total)
    } else {
        0.0
    };
    let cap = cfg.thresholds.fix_ratio.cap;
    let score = ((1.0 - ratio / cap) * PERCENT).round().clamp(0.0, PERCENT);

    FixRatioScore {
        score,
        fix,
        total,
        ratio,
    }
}

#[cfg(test)]
mod tests {
    //! Covers a zero-FIX baseline, the score floor once FIX ratio hits the cap, a mid-range ratio, the
    //! no-nodes/no-tags baseline, and aggregation of fix/total counts across multiple nodes.
    use super::*;
    use std::collections::HashMap;

    fn node(tag_counts: &[(&str, u32)]) -> FileNode {
        FileNode {
            id: "x".to_string(),
            path: "x".to_string(),
            change_count: 0,
            churn: 0,
            last_modified: None,
            author_count: 1,
            loc: 10,
            tag_counts: tag_counts
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect::<HashMap<_, _>>(),
            fan_in: 0,
            fan_out: 0,
            total_connections: 0,
            risk_score: 0.0,
            ..Default::default()
        }
    }

    fn cfg() -> ScoresConfig {
        ScoresConfig::default()
    }

    #[test]
    fn fix_count_0_score_100() {
        let r = compute_fix_ratio(&[node(&[("ADD", 10), ("UPDATE", 5)])], &cfg());
        assert_eq!(r.score, 100.0);
        assert_eq!(r.fix, 0);
        assert_eq!(r.ratio, 0.0);
    }

    #[test]
    fn fix_at_least_30_percent_score_0_capped() {
        let r = compute_fix_ratio(&[node(&[("FIX", 5), ("ADD", 5)])], &cfg());
        assert_eq!(r.score, 0.0);
        assert_eq!(r.ratio, 0.5);
    }

    #[test]
    fn fix_10_percent_approx_67() {
        // (1 - 0.1/0.3) * 100
        let r = compute_fix_ratio(&[node(&[("FIX", 1), ("ADD", 9)])], &cfg());
        assert_eq!(r.score, 67.0);
    }

    #[test]
    fn no_nodes_or_no_tags_score_100() {
        assert_eq!(compute_fix_ratio(&[], &cfg()).score, 100.0);
        assert_eq!(compute_fix_ratio(&[node(&[])], &cfg()).score, 100.0);
    }

    #[test]
    fn aggregates_fix_total_across_multiple_nodes() {
        let r = compute_fix_ratio(
            &[
                node(&[("FIX", 2), ("ADD", 3)]),
                node(&[("FIX", 1), ("ADD", 4)]),
            ],
            &cfg(),
        );
        assert_eq!(r.fix, 3);
        assert_eq!(r.total, 10);
        assert_eq!(r.ratio, 0.3);
        assert_eq!(r.score, 0.0);
    }
}
