//! God file — detects files exceeding twice the SFC LOC limit. Where `sfc` flags any file over the
//! per-role convention, `god_file` flags the more severe case of a file at 2x that limit — a stronger
//! single-responsibility violation signal, penalized more steeply per offending file.
//!
//! Non-source files (data/config/assets with real `loc` but no parser frontend — a `pnpm-lock.yaml`, a
//! `.css`, a huge generated `.json`) are excluded from BOTH the violation list AND the live/limit
//! denominator, not merely never-flagged: their size is not a single-responsibility signal, so counting
//! them as compliant "live" files would silently inflate the compliant ratio too. Source-ness is injected
//! as an `is_source` closure (mirroring `zpz_core::build_file_nodes`) — this module never hardcodes
//! language/extension knowledge; the engine's dispatch table is the single source of truth, passed in.

use super::config::ScoresConfig;
use super::types::{GodFile, GodFileScore};
use zpz_core::FileNode;

/// Detail list cap.
const MAX_DETAIL_ITEMS: usize = 50;

pub fn compute_god_file<F>(
    nodes: &[FileNode],
    target: Option<&str>,
    cfg: &ScoresConfig,
    is_source: F,
) -> GodFileScore
where
    F: Fn(&str) -> bool,
{
    let sfc_limit = cfg.thresholds.loc_limit(target);
    let limit = (sfc_limit as f64 * cfg.thresholds.god_file.loc_multiplier).round() as u32;
    let live: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| n.loc > 0 && is_source(&n.id))
        .collect();
    let mut gods: Vec<GodFile> = live
        .iter()
        .filter(|n| n.loc > limit)
        .map(|n| GodFile {
            path: n.path.clone(),
            loc: n.loc,
        })
        .collect();
    gods.sort_by_key(|g| std::cmp::Reverse(g.loc));
    let score = if live.is_empty() {
        100.0
    } else {
        (100.0 - (gods.len() as f64 / live.len() as f64) * cfg.thresholds.god_file.penalty_slope)
            .max(0.0)
    };
    gods.truncate(MAX_DETAIL_ITEMS);
    GodFileScore {
        score: score.round(),
        limit,
        files: gods,
    }
}

#[cfg(test)]
mod tests {
    //! Covers the no-live-files baseline, all files under the limit, a single god file among several,
    //! the score floor at 0, and the per-target limit override — all against `ScoresConfig::default()`.
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
    fn no_live_files_score_100_default_limit_300() {
        // no target -> sfcLimit 150 -> limit 300
        let r = compute_god_file(&[], None, &ScoresConfig::default(), |_| true);
        assert_eq!(r.score, 100.0);
        assert_eq!(r.limit, 300);
        assert_eq!(r.files, vec![]);
    }

    #[test]
    fn all_files_under_limit_score_100() {
        // default limit 300; both <= 300
        let nodes = [node("a", 100), node("b", 300)];
        let r = compute_god_file(&nodes, None, &ScoresConfig::default(), |_| true);
        assert_eq!(r.score, 100.0);
        assert_eq!(r.files, vec![]);
    }

    #[test]
    fn one_god_file_out_of_four_score_50() {
        // limit 300, gods=1, live=4 -> 100 - (1/4)*200 = 100 - 50 = 50
        let nodes = [
            node("god", 500),
            node("a", 100),
            node("b", 100),
            node("c", 100),
        ];
        let r = compute_god_file(&nodes, None, &ScoresConfig::default(), |_| true);
        assert_eq!(r.score, 50.0);
        assert_eq!(
            r.files,
            vec![GodFile {
                path: "god".to_string(),
                loc: 500
            }]
        );
    }

    #[test]
    fn half_of_files_are_god_files_score_floors_at_0() {
        // gods=1, live=2 -> 100 - (1/2)*200 = 0
        let nodes = [node("god", 700), node("ok", 50)];
        let r = compute_god_file(&nodes, None, &ScoresConfig::default(), |_| true);
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn target_fe_lowers_the_limit_100_to_200() {
        // target fe -> sfcLimit 100 -> limit 200; loc 250 > 200 is a god file
        let nodes = [node("big", 250)];
        let r = compute_god_file(&nodes, Some("fe"), &ScoresConfig::default(), |_| true);
        assert_eq!(r.limit, 200);
        assert_eq!(
            r.files,
            vec![GodFile {
                path: "big".to_string(),
                loc: 250
            }]
        );
        // gods=1, live=1 -> 100 - 200 = -100 -> 0
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn non_source_huge_file_excluded_from_violations_and_denominator() {
        // A huge non-source file (e.g. a lockfile) must not appear as a violation NOR count toward the
        // live/limit denominator — it is treated as absent from the metric entirely, so a lone compliant
        // source file still scores a perfect 100.
        let nodes = [node("pnpm-lock.yaml", 5174), node("src/app.ts", 100)];
        let r = compute_god_file(&nodes, None, &ScoresConfig::default(), |id| {
            id == "src/app.ts"
        });
        assert_eq!(r.files, vec![]);
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn same_huge_file_classified_as_source_still_triggers_the_violation() {
        // Same 5174 LOC, but now source: the gate is about source-ness, not about magically raising the
        // limit, so a source file of that size still lands in `files` as a god file.
        let nodes = [node("src/huge.ts", 5174)];
        let r = compute_god_file(&nodes, None, &ScoresConfig::default(), |_| true);
        assert_eq!(
            r.files,
            vec![GodFile {
                path: "src/huge.ts".to_string(),
                loc: 5174
            }]
        );
    }
}
