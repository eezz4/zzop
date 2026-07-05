//! File-pair coupling counts from commit co-change history — coupling\[a\]\[b\] = number of commits
//! that touched both a and b (symmetric). Commits exceeding MAX_FILES_PER_COMMIT files are skipped
//! to suppress large-refactor noise.
//!
//! `CommitFileSet` (this module's input type) stays in `zpz_core` — it is shared IR, constructed by
//! `zpz_git` and consumed directly by `zpz_engine` — per the crate-boundary split: shared IR stays in
//! core even as its downstream computation moves to a dedicated crate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use zpz_core::CommitFileSet;

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
