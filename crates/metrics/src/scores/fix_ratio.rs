//! FIX-work share — how much of the tagged history is reactive fixing. Lower means fewer bugs found
//! after the fact, relative to the rest of the tagged work.
//!
//! # The unit is a FILE TOUCH, not a commit
//! This module doc used to say "proportion of `[FIX]`-tagged commits out of all commits" and that was
//! false on BOTH terms — it was the source the field names `fix`/`total`/`ratio` were read against, so
//! the names and the doc were corrected together on 2026-07-31 (user ruling; the computation is
//! unchanged). What the loop below actually sums is `FileNode::tag_counts`, which records a commit's
//! type tag once per FILE that commit touched:
//!
//! - a `[FIX]` commit touching three scored files adds THREE, not one. Wide fixes therefore weigh more
//!   than narrow ones — arguably the more useful signal for a per-file health score, but it is not a
//!   commit count and must not be named like one.
//! - a commit matching NO commit-type pattern adds ZERO, to neither side. The denominator is the TAGGED
//!   touches, so "all commits" was never the denominator either. `zzop_metrics::diagnostics` already
//!   warns when a repo's commits tag at 0%, which is the same fact seen from the other end.
//! - only files passing `is_scored` contribute at all (`exclude`d paths leave the violation list and the
//!   denominator together — the two-sided rule the excludes design applies across the score channels).
//!
//! The score maps that share through a configured cap: `score = clamp(round((1 - share/cap) * 100),
//! 0, 100)`.

use crate::scores::config::ScoresConfig;
use crate::scores::types::FixRatioScore;
use zzop_core::FileNode;

/// The 0-100 score scale.
const PERCENT: f64 = 100.0;

/// Sums `FileNode::tag_counts` over the scored files — see the module doc for what one unit of that sum
/// is. The share is 0 when nothing was tagged at all, which scores 100: a repo whose commits carry no
/// type tags is indistinguishable here from one with no fixes, and `diagnostics` is the channel that
/// says so rather than this number pretending to.
pub fn compute_fix_ratio(
    nodes: &[FileNode],
    cfg: &ScoresConfig,
    is_scored: &dyn Fn(&str) -> bool,
) -> FixRatioScore {
    let mut fix_file_touches: u32 = 0;
    let mut tagged_file_touches: u32 = 0;
    for n in nodes.iter().filter(|n| is_scored(&n.id)) {
        for (tag, count) in &n.tag_counts {
            tagged_file_touches += count;
            if tag == "FIX" {
                fix_file_touches += count;
            }
        }
    }

    let fix_share_of_tagged_touches = if tagged_file_touches > 0 {
        f64::from(fix_file_touches) / f64::from(tagged_file_touches)
    } else {
        0.0
    };
    let cap = cfg.thresholds.fix_ratio.cap;
    let score = ((1.0 - fix_share_of_tagged_touches / cap) * PERCENT)
        .round()
        .clamp(0.0, PERCENT);

    FixRatioScore {
        score,
        fix_file_touches,
        tagged_file_touches,
        fix_share_of_tagged_touches,
    }
}

#[cfg(test)]
mod tests {
    //! Covers a zero-FIX baseline, the score floor once the FIX share hits the cap, a mid-range share,
    //! the no-nodes/no-tags baseline, aggregation across multiple nodes, and — the two the field names
    //! now claim — that one commit spanning N files counts N, and that an untagged commit counts on
    //! neither side.
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
        let r = compute_fix_ratio(&[node(&[("ADD", 10), ("UPDATE", 5)])], &cfg(), &|_| true);
        assert_eq!(r.score, 100.0);
        assert_eq!(r.fix_file_touches, 0);
        assert_eq!(r.fix_share_of_tagged_touches, 0.0);
    }

    #[test]
    fn fix_at_least_30_percent_score_0_capped() {
        let r = compute_fix_ratio(&[node(&[("FIX", 5), ("ADD", 5)])], &cfg(), &|_| true);
        assert_eq!(r.score, 0.0);
        assert_eq!(r.fix_share_of_tagged_touches, 0.5);
    }

    #[test]
    fn fix_10_percent_approx_67() {
        // (1 - 0.1/0.3) * 100
        let r = compute_fix_ratio(&[node(&[("FIX", 1), ("ADD", 9)])], &cfg(), &|_| true);
        assert_eq!(r.score, 67.0);
    }

    #[test]
    fn no_nodes_or_no_tags_score_100() {
        assert_eq!(compute_fix_ratio(&[], &cfg(), &|_| true).score, 100.0);
        assert_eq!(
            compute_fix_ratio(&[node(&[])], &cfg(), &|_| true).score,
            100.0
        );
    }

    #[test]
    fn aggregates_touches_across_multiple_nodes() {
        let r = compute_fix_ratio(
            &[
                node(&[("FIX", 2), ("ADD", 3)]),
                node(&[("FIX", 1), ("ADD", 4)]),
            ],
            &cfg(),
            &|_| true,
        );
        assert_eq!(r.fix_file_touches, 3);
        assert_eq!(r.tagged_file_touches, 10);
        assert_eq!(r.fix_share_of_tagged_touches, 0.3);
        assert_eq!(r.score, 0.0);
    }

    /// The rule `fix_file_touches` is named for. ONE `[FIX]` commit that touched three files leaves a
    /// `FIX` tag on each of those three `FileNode`s, so it counts THREE — a commit count would say one.
    /// Pinned because the old name `fix` plus a module doc reading "fix commits" made the wrong number
    /// the obvious reading, and nothing in the suite contradicted it.
    #[test]
    fn one_commit_spanning_three_files_counts_three_touches() {
        let one_fix_commit_over_three_files = [
            node(&[("FIX", 1)]),
            node(&[("FIX", 1)]),
            node(&[("FIX", 1)]),
        ];
        let r = compute_fix_ratio(&one_fix_commit_over_three_files, &cfg(), &|_| true);
        assert_eq!(r.fix_file_touches, 3);
        assert_eq!(r.tagged_file_touches, 3);
    }

    /// The rule `tagged_file_touches` is named for. A commit matching no commit-type pattern leaves no
    /// entry in `tag_counts`, so it lands in NEITHER side of the share — the denominator is the tagged
    /// history, never "all commits". Here the untagged file is invisible and the share is the tagged
    /// file's alone; had the denominator been all commits it would have been half of this.
    #[test]
    fn an_untagged_commit_enters_neither_side_of_the_share() {
        let r = compute_fix_ratio(&[node(&[("FIX", 1)]), node(&[])], &cfg(), &|_| true);
        assert_eq!(r.tagged_file_touches, 1);
        assert_eq!(r.fix_share_of_tagged_touches, 1.0);
    }
}
