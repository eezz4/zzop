//! File-pair coupling counts from commit co-change history — coupling\[a\]\[b\] = number of commits
//! that touched both a and b (symmetric). Commits exceeding MAX_FILES_PER_COMMIT files are skipped
//! to suppress large-refactor noise.
//!
//! `CommitFileSet` (this module's input type) stays in `zzop_core` — it is shared IR, constructed by
//! `zzop_git` and consumed directly by `zzop_engine` — per the crate-boundary split: shared IR stays in
//! core even as its downstream computation moves to a dedicated crate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use zzop_core::CommitFileSet;

/// A commit must touch at least this many files to form a co-change pair (1-file commits couple nothing).
pub const MIN_FILES_PER_COMMIT: usize = 2;
/// Commits touching more than this many files are skipped as large-refactor noise.
pub const MAX_FILES_PER_COMMIT: usize = 25;
/// Default cap on coupled partners kept per file.
pub const COUPLING_TOP_PER_FILE: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CouplingEntry {
    pub path: String,
    pub count: u32,
}

/// path -> top coupled files (count desc; ties by path for deterministic output).
pub type CouplingMap = BTreeMap<String, Vec<CouplingEntry>>;

pub fn build_coupling(commits: &[CommitFileSet], top_per_file: usize) -> CouplingMap {
    let mut pair_counts: BTreeMap<&str, BTreeMap<&str, u32>> = BTreeMap::new();

    for c in commits {
        if c.files.len() < MIN_FILES_PER_COMMIT || c.files.len() > MAX_FILES_PER_COMMIT {
            continue;
        }
        for i in 0..c.files.len() {
            for j in (i + 1)..c.files.len() {
                increment(&mut pair_counts, &c.files[i], &c.files[j]);
                increment(&mut pair_counts, &c.files[j], &c.files[i]);
            }
        }
    }

    let mut result = CouplingMap::new();
    for (path, partners) in pair_counts {
        let mut entries: Vec<CouplingEntry> = partners
            .into_iter()
            .map(|(p, count)| CouplingEntry {
                path: p.to_string(),
                count,
            })
            .collect();
        // Stable sort over path-ordered entries: count desc, ties stay in path order.
        entries.sort_by_key(|e| std::cmp::Reverse(e.count));
        entries.truncate(top_per_file);
        result.insert(path.to_string(), entries);
    }
    result
}

fn increment<'a>(map: &mut BTreeMap<&'a str, BTreeMap<&'a str, u32>>, from: &'a str, to: &'a str) {
    *map.entry(from).or_default().entry(to).or_insert(0) += 1;
}

/// One undirected co-change pair. `a < b` always, so a pair has exactly one representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoChangeEdge {
    pub a: String,
    pub b: String,
    /// Commits that touched both — the same count [`CouplingEntry`] carries.
    pub count: u32,
}

/// [`CouplingMap`] flattened to undirected edges — the shape a graph consumer needs, and a PROJECTION
/// of the map rather than a second computation, so the picture and the `seams`/`recommendations` scores
/// can never disagree about what co-changed.
///
/// **The map is asymmetric and this reconciles it.** Each file keeps only its own top
/// [`COUPLING_TOP_PER_FILE`] partners, so `a` may keep `b` while `b` — with more partners of its own —
/// dropped `a`. An edge is emitted when EITHER side kept it, which is the direction that loses less:
/// requiring both would silently delete every tie a busy file has to a quiet one, and those are exactly
/// the ties worth seeing. The two sides agree on `count` when both kept it (the underlying pair counter
/// is symmetric), so the max is a tie-break that never fires rather than a reconciliation.
///
/// ⚠ **This is a doubly filtered sample, never a repository total.** Commits outside
/// [`MIN_FILES_PER_COMMIT`]..=[`MAX_FILES_PER_COMMIT`] form no pair at all, and the per-file top-N drops
/// the tail rather than summing it. Read it as "the strongest measured co-change", the same reading
/// `crate::seams::SeamCandidate::temporal_boundary` documents for its own use of this substrate.
pub fn co_change_edges(coupling: &CouplingMap) -> Vec<CoChangeEdge> {
    let mut best: BTreeMap<(&str, &str), u32> = BTreeMap::new();
    for (path, partners) in coupling {
        for entry in partners {
            let (a, b) = if path.as_str() < entry.path.as_str() {
                (path.as_str(), entry.path.as_str())
            } else {
                (entry.path.as_str(), path.as_str())
            };
            if a == b {
                continue; // a file cannot co-change with itself; defensive, `build_coupling` never emits it
            }
            let slot = best.entry((a, b)).or_insert(0);
            *slot = (*slot).max(entry.count);
        }
    }
    let mut edges: Vec<CoChangeEdge> = best
        .into_iter()
        .map(|((a, b), count)| CoChangeEdge {
            a: a.to_string(),
            b: b.to_string(),
            count,
        })
        .collect();
    // Count desc, then the BTreeMap's (a, b) order — deterministic output, strongest first.
    edges.sort_by(|x, y| {
        y.count
            .cmp(&x.count)
            .then_with(|| (&x.a, &x.b).cmp(&(&y.a, &y.b)))
    });
    edges
}

#[cfg(test)]
mod tests {
    //! Exercises file-pair coupling accumulation from commit co-change history.
    use super::*;

    fn commit(sha: &str, files: &[&str]) -> CommitFileSet {
        CommitFileSet {
            sha: sha.into(),
            files: files.iter().map(|s| s.to_string()).collect(),
            tags: vec![],
            date: None,
            subject: None,
            labels: vec![],
        }
    }

    #[test]
    fn files_in_same_commit_increment_coupling_count() {
        let m = build_coupling(
            &[
                commit("1", &["a.ts", "b.ts"]),
                commit("2", &["a.ts", "b.ts"]),
                commit("3", &["a.ts", "c.ts"]),
            ],
            COUPLING_TOP_PER_FILE,
        );
        let a_partners = &m["a.ts"];
        assert_eq!(
            a_partners[0],
            CouplingEntry {
                path: "b.ts".into(),
                count: 2
            }
        );
        assert_eq!(
            a_partners[1],
            CouplingEntry {
                path: "c.ts".into(),
                count: 1
            }
        );
    }

    #[test]
    fn large_commits_are_skipped_as_noise() {
        let big: Vec<String> = (0..30).map(|i| format!("f{i}.ts")).collect();
        let big_refs: Vec<&str> = big.iter().map(|s| s.as_str()).collect();
        let m = build_coupling(&[commit("1", &big_refs)], COUPLING_TOP_PER_FILE);
        assert!(!m.contains_key("f0.ts"));
    }

    #[test]
    fn single_file_commits_produce_no_coupling() {
        let m = build_coupling(&[commit("1", &["a.ts"])], COUPLING_TOP_PER_FILE);
        assert!(!m.contains_key("a.ts"));
    }
}

#[cfg(test)]
mod co_change_edge_tests {
    use super::*;

    fn c(sha: &str, files: &[&str]) -> CommitFileSet {
        CommitFileSet {
            sha: sha.into(),
            files: files.iter().map(|s| s.to_string()).collect(),
            tags: vec![],
            date: None,
            subject: None,
            labels: vec![],
        }
    }

    #[test]
    fn a_pair_appears_once_with_a_stable_orientation() {
        let edges = co_change_edges(&build_coupling(&[c("1", &["b.rs", "a.rs"])], 10));
        assert_eq!(
            edges,
            vec![CoChangeEdge {
                a: "a.rs".into(),
                b: "b.rs".into(),
                count: 1
            }],
            "the map holds both directions; the edge list must not"
        );
    }

    #[test]
    fn an_edge_survives_when_only_the_quiet_side_kept_it() {
        // The asymmetry this function exists to reconcile: `hub` has more partners than it may keep, so
        // its own list drops `quiet` — but `quiet`'s list still names `hub`, and that tie is real.
        let mut commits = vec![c("pair", &["hub.rs", "quiet.rs"])];
        for i in 0..5 {
            commits.push(c(&format!("busy{i}"), &["hub.rs", &format!("other{i}.rs")]));
            commits.push(c(
                &format!("busy{i}b"),
                &["hub.rs", &format!("other{i}.rs")],
            ));
        }
        let coupling = build_coupling(&commits, 2); // hub keeps only its two strongest
        assert!(
            !coupling["hub.rs"].iter().any(|e| e.path == "quiet.rs"),
            "setup: hub must have dropped quiet"
        );
        let edges = co_change_edges(&coupling);
        assert!(
            edges
                .iter()
                .any(|e| e.a == "hub.rs" && e.b == "quiet.rs" && e.count == 1),
            "an edge kept by one side only must survive: {edges:?}"
        );
    }

    #[test]
    fn edges_are_ordered_strongest_first_and_deterministically() {
        let commits = vec![
            c("1", &["a.rs", "b.rs"]),
            c("2", &["a.rs", "b.rs"]),
            c("3", &["c.rs", "d.rs"]),
        ];
        let edges = co_change_edges(&build_coupling(&commits, 10));
        assert_eq!(edges[0].count, 2, "strongest first: {edges:?}");
        assert_eq!(
            edges,
            co_change_edges(&build_coupling(&commits, 10)),
            "same input must give byte-identical output"
        );
    }

    #[test]
    fn a_git_less_run_yields_no_edges_rather_than_a_wrong_zero() {
        // The empty map is what a run with no commits produces. The DISTINCTION between "measured, none
        // found" and "not measured" is not this function's to make — it is carried by the output field
        // being `Option`, which is why this only pins that the empty case is empty and never panics.
        assert!(co_change_edges(&CouplingMap::new()).is_empty());
    }
}
