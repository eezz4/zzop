//! Bus factor — ratio of "live" high-churn files (`change_count >= min_live_changes`) with a single author.
//! A single-author, frequently-changed file is a knowledge-isolation risk: nobody else understands it, so its
//! author leaving is a real bus-factor-1 hazard.

use crate::scores::config::ScoresConfig;
use crate::scores::types::{BusFactorFile, BusFactorScore};
use zpz_core::FileNode;

/// Max detail rows returned.
const MAX_DETAIL_ITEMS: usize = 50;
/// The 0-100 score scale.
const PERCENT: f64 = 100.0;

/// `score = 100 - (risky / total) * 100`, where `total` = live (loc>0, change_count >= min_live_changes) files and
/// `risky` = live files additionally restricted to a single author. 100 when there are no eligible (live) files.
pub fn compute_bus_factor(nodes: &[FileNode], cfg: &ScoresConfig) -> BusFactorScore {
    let min_live_changes = cfg.thresholds.bus_factor.min_live_changes;

    let live: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| n.loc > 0 && n.change_count >= min_live_changes && n.author_count == 1)
        .collect();

    let mut files: Vec<BusFactorFile> = live
        .iter()
        .map(|n| BusFactorFile {
            path: n.path.clone(),
            change_count: n.change_count,
            authors: n.author_count,
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.change_count));

    let total = nodes
        .iter()
        .filter(|n| n.loc > 0 && n.change_count >= min_live_changes)
        .count() as u32;
    let risky = live.len() as u32;

    let score = if total == 0 {
        PERCENT
    } else {
        (PERCENT - (f64::from(risky) / f64::from(total)) * PERCENT).max(0.0)
    };

    files.truncate(MAX_DETAIL_ITEMS);

    BusFactorScore {
        score: score.round(),
        risky,
        files,
    }
}

#[cfg(test)]
mod tests {
    //! Covers empty input, no eligible files, all/half of eligible files being single-author, and files with
    //! zero LOC being excluded from both the live and total counts.
    use super::*;

    fn node(path: &str, loc: u32, change_count: u32, author_count: u32) -> FileNode {
        FileNode {
            id: path.to_string(),
            path: path.to_string(),
            change_count,
            churn: 0,
            last_modified: None,
            author_count,
            loc,
            tag_counts: Default::default(),
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
    fn no_eligible_files_change_count_below_10_score_100() {
        let r = compute_bus_factor(&[node("x", 10, 5, 1)], &cfg());
        assert_eq!(r.score, 100.0);
        assert_eq!(r.risky, 0);
        assert!(r.files.is_empty());
    }

    #[test]
    fn empty_input_score_100() {
        let r = compute_bus_factor(&[], &cfg());
        assert_eq!(r.score, 100.0);
        assert_eq!(r.risky, 0);
    }

    #[test]
    fn all_eligible_files_single_author_score_0() {
        // live = total = 2 -> 100 - (2/2)*100 = 0
        let r = compute_bus_factor(&[node("a", 10, 12, 1), node("b", 10, 20, 1)], &cfg());
        assert_eq!(r.score, 0.0);
        assert_eq!(r.risky, 2);
        // sorted by change_count desc
        let paths: Vec<&str> = r.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["b", "a"]);
    }

    #[test]
    fn half_of_eligible_files_are_single_author_score_50() {
        // total = 2 (both change_count>=10), live = 1 (single-author) -> 100 - (1/2)*100 = 50
        let r = compute_bus_factor(&[node("solo", 10, 15, 1), node("team", 10, 15, 3)], &cfg());
        assert_eq!(r.score, 50.0);
        assert_eq!(r.risky, 1);
        let paths: Vec<&str> = r.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["solo"]);
    }

    #[test]
    fn loc_0_files_are_excluded_from_both_live_and_total() {
        // the loc:0 single-author churny file is ignored; only the team file counts toward total
        let r = compute_bus_factor(&[node("ghost", 0, 30, 1), node("team", 50, 30, 4)], &cfg());
        // total = 1, live = 0 -> 100
        assert_eq!(r.score, 100.0);
        assert_eq!(r.risky, 0);
    }
}
