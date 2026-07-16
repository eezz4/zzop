//! zzop-git — git history collection: ONE `git log --numstat` pass produces both per-file `GitStats`
//! (zzop_core::file_nodes) and per-commit `CommitFileSet`s (zzop_core::coupling) together, in the same
//! streaming parse. No consumer re-runs git per file or per commit. The design is single-pass by
//! construction: one numstat traversal yields rename tracking (`alias_to_canonical`), the HEAD-hash
//! cache key, and the recent-activity window, so collection cost is independent of file count.
//!
//! [`parse_git_log`] is the pure core — it never touches git or the filesystem, so it is fully
//! testable against canned `git log` text. [`collect`] and [`head_hash`] are the two
//! `std::process::Command` entry points that feed it (never more than one git process per call).

mod error;
mod iso_date;
mod parse;
mod process;
mod tags;

use std::path::Path;

pub use error::GitError;
pub use parse::parse_git_log;

use zzop_core::{CommitFileSet, GitStats};

/// One `git log --numstat` pass's output: per-file stats + per-commit file sets + the covered window.
#[derive(Debug, Clone, Default)]
pub struct GitCollection {
    pub stats: GitStats,
    pub commits: Vec<CommitFileSet>,
    pub window: GitWindow,
}

/// The git history window an analysis covered — makes churn/tag/lifecycle numbers interpretable (they
/// mean very different things over 90 days vs. full history) and lets a caller detect an unbounded
/// walk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitWindow {
    /// The `--since` value passed to `collect`; `None` = full history.
    pub since: Option<String>,
    /// ISO date of the oldest commit in range (`None` when there are no commits).
    pub first: Option<String>,
    /// ISO date of the newest commit in range.
    pub last: Option<String>,
    /// Commits in the analyzed range.
    pub commits: usize,
}

/// Options for [`collect`] / [`parse_git_log`].
#[derive(Debug, Clone)]
pub struct CollectOptions {
    /// `git log --since=<since>`; `None` = full history.
    pub since: Option<String>,
    /// Window, in days, for the `recent_*` fields on each `GitStats` entry. Default 30 — shared with
    /// `zzop_core::DEFAULT_RECENT_THRESHOLD_DAYS` (the lifecycle classifier's own recency window), so a
    /// file's "recent" churn numbers line up with the same window the lifecycle classifier uses to call
    /// it recently active.
    pub recent_days: u32,
    /// Ordered `(regex, TAG)` commit-type classifiers, applied only when a commit subject has no
    /// explicit `[TAG]` bracket. Default: empty — bracket tags (`[FIX]`, `[FEAT]`, ...) still classify,
    /// but no keyword vocabulary is assumed. This crate collects; it does not decide what a "FIX" commit
    /// looks like (see `tags`'s module doc) — a caller wanting the default FIX/FEAT/... keyword table
    /// supplies `zzop_metrics::default_commit_type_patterns()` here (as `zzop_engine::analyze::collect_git`
    /// does), or its own project-specific vocabulary.
    pub commit_type_patterns: Vec<(String, String)>,
}

impl Default for CollectOptions {
    fn default() -> Self {
        CollectOptions {
            since: None,
            recent_days: 30,
            commit_type_patterns: Vec::new(),
        }
    }
}

/// Collects git history for the repository at `repo`: one `git log --numstat` process call, parsed by
/// [`parse_git_log`]. Returns a typed [`GitError`] (never panics) when `repo` has no git available, is
/// not inside a repository, or the invocation otherwise fails.
pub fn collect(repo: &Path, opts: &CollectOptions) -> Result<GitCollection, GitError> {
    let output = process::run_git_log(repo, opts)?;
    Ok(parse_git_log(&output, opts, now_ms()))
}

/// The repository's current `HEAD` commit hash — a cache-key input (an unchanged HEAD means the
/// history is unchanged, so a caller can skip re-collecting). One `git rev-parse HEAD` process call.
pub fn head_hash(repo: &Path) -> Result<String, GitError> {
    process::head_hash_impl(repo)
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
