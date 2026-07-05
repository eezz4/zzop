//! Rename instability — ratio of files with `rename_count > 0`. Pure over `FileNode`s (no config needed): a file
//! that keeps getting renamed/moved has an unstable location, which is a mild structural smell (import churn,
//! broken links, review noise).

use crate::scores::types::{RenameScore, RenamedFile};
use zzop_core::FileNode;

/// Max detail rows returned.
const MAX_DETAIL_ITEMS: usize = 50;
/// The 0-100 score scale.
const PERCENT: f64 = 100.0;

/// `score = 100 - (renamed / live) * 100`, where `live` = files with `loc > 0` and `renamed` = live files with
/// `rename_count > 0`. 100 when there are no live files (nothing to penalize).
pub fn compute_rename(nodes: &[FileNode]) -> RenameScore {
    let live: Vec<&FileNode> = nodes.iter().filter(|n| n.loc > 0).collect();

    let mut files: Vec<RenamedFile> = live
        .iter()
        .filter(|n| n.rename_count.unwrap_or(0) > 0)
        .map(|n| RenamedFile {
            path: n.path.clone(),
            rename_count: n.rename_count.unwrap_or(0),
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.rename_count));

    let renamed = files.len() as u32;
    let total = live.len() as u32;
    let score = if live.is_empty() {
        PERCENT
    } else {
        (PERCENT - (f64::from(renamed) / f64::from(total)) * PERCENT).max(0.0)
    };

    files.truncate(MAX_DETAIL_ITEMS);

    RenameScore {
        score: score.round(),
        renamed,
        total,
        files,
    }
}

#[cfg(test)]
mod tests {
    //! Covers the no-live-files baseline, no renames (undefined or zero count), a partial-renamed case,
    //! all files renamed (sorted by rename count descending), and zero-LOC files being excluded from
    //! both the total and renamed counts.
    use super::*;

    fn node(path: &str, loc: u32, rename_count: Option<u32>) -> FileNode {
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
            rename_count,
            ..Default::default()
        }
    }

    #[test]
    fn no_live_files_score_100() {
        let r = compute_rename(&[]);
        assert_eq!(r.score, 100.0);
        assert_eq!(r.renamed, 0);
        assert_eq!(r.total, 0);
        assert!(r.files.is_empty());
    }

    #[test]
    fn no_renames_rename_count_undefined_or_zero_score_100() {
        let r = compute_rename(&[node("a", 10, None), node("b", 10, Some(0))]);
        assert_eq!(r.score, 100.0);
        assert_eq!(r.renamed, 0);
        assert_eq!(r.total, 2);
    }

    #[test]
    fn one_renamed_out_of_four_score_75() {
        // renamed=1, live=4 -> 100 - (1/4)*100 = 75
        let r = compute_rename(&[
            node("moved", 10, Some(2)),
            node("a", 10, None),
            node("b", 10, None),
            node("c", 10, None),
        ]);
        assert_eq!(r.score, 75.0);
        assert_eq!(r.renamed, 1);
        assert_eq!(r.total, 4);
        assert_eq!(
            r.files,
            vec![RenamedFile {
                path: "moved".to_string(),
                rename_count: 2
            }]
        );
    }

    #[test]
    fn all_files_renamed_score_0_sorted_by_rename_count_desc() {
        // renamed=2, live=2 -> 100 - 100 = 0
        let r = compute_rename(&[node("a", 10, Some(1)), node("b", 10, Some(3))]);
        assert_eq!(r.score, 0.0);
        let paths: Vec<&str> = r.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["b", "a"]);
    }

    #[test]
    fn loc_0_files_excluded_from_total_and_renamed() {
        // ghost (loc 0) renamed is ignored; only the live un-renamed file counts
        let r = compute_rename(&[node("ghost", 0, Some(5)), node("real", 20, None)]);
        assert_eq!(r.total, 1);
        assert_eq!(r.renamed, 0);
        assert_eq!(r.score, 100.0);
    }
}
