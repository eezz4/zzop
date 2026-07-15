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
mod tests {
    //! `parse_git_log` tests against canned `git log --numstat` text using this crate's format (see
    //! `process::run_git_log`): `__C__<sha>\x1f<isoDate>\x1f<author>\x1f<subject>` header lines,
    //! numstat lines (`added\tdeleted\tpath`, renames as `old => new` / `{old => new}`, binary as
    //! `-\t-\tpath`). These cases cover `parse_git_log`'s documented semantics: rename merging, binary
    //! exclusion, multi-author aggregation, tag classification (incl. REVERT-before-FIX), and the
    //! recent-window boundary; the tag-classifier cases specifically live in `tags.rs`.
    use super::*;

    const SEP: char = '\u{1f}';

    fn header(sha: &str, date: &str, author: &str, subject: &str) -> String {
        format!("__C__{sha}{SEP}{date}{SEP}{author}{SEP}{subject}")
    }

    fn opts() -> CollectOptions {
        CollectOptions::default()
    }

    /// A fixed "now" far enough past every fixture date that nothing in the basic fixtures counts as
    /// recent unless a test explicitly asks for it.
    const FAR_FUTURE_NOW_MS: i64 = 4_102_444_800_000; // 2100-01-01T00:00:00Z

    #[test]
    fn single_commit_single_file_accumulates_change_and_churn() {
        let log = format!(
            "{}\n10\t2\tsrc/a.ts\n",
            header("sha1", "2026-01-01T00:00:00Z", "a@x.com", "add feature")
        );
        let result = parse_git_log(&log, &opts(), FAR_FUTURE_NOW_MS);
        let a = result.stats.by_path.get("src/a.ts").unwrap();
        assert_eq!(a.change_count, 1);
        assert_eq!(a.churn, 12);
        assert_eq!(a.author_count, 1);
        assert_eq!(a.last_modified.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(result.commits.len(), 1);
        assert_eq!(result.commits[0].files, vec!["src/a.ts".to_string()]);
    }

    #[test]
    fn multiple_commits_accumulate_and_track_last_modified() {
        let log = format!(
            "{}\n5\t1\tsrc/a.ts\n{}\n3\t0\tsrc/a.ts\n",
            header("sha1", "2026-01-01T00:00:00Z", "a@x.com", "init"),
            header("sha2", "2026-02-01T00:00:00Z", "b@x.com", "update")
        );
        let result = parse_git_log(&log, &opts(), FAR_FUTURE_NOW_MS);
        let a = result.stats.by_path.get("src/a.ts").unwrap();
        assert_eq!(a.change_count, 2);
        assert_eq!(a.churn, 9);
        assert_eq!(a.author_count, 2);
        assert_eq!(a.last_modified.as_deref(), Some("2026-02-01T00:00:00Z"));
        assert_eq!(
            a.author_commits.as_ref().unwrap().get("a@x.com").copied(),
            Some(1)
        );
        assert_eq!(
            a.author_commits.as_ref().unwrap().get("b@x.com").copied(),
            Some(1)
        );
    }

    #[test]
    fn top_level_rename_merges_old_path_stats_into_new_canonical_path() {
        let log = format!(
            "{}\n5\t1\tsrc/old.ts\n{}\n2\t0\tsrc/old.ts => src/new.ts\n",
            header("sha1", "2026-01-01T00:00:00Z", "a@x.com", "init"),
            header("sha2", "2026-02-01T00:00:00Z", "a@x.com", "rename")
        );
        let result = parse_git_log(&log, &opts(), FAR_FUTURE_NOW_MS);
        assert!(!result.stats.by_path.contains_key("src/old.ts"));
        let new_file = result.stats.by_path.get("src/new.ts").unwrap();
        assert_eq!(new_file.change_count, 2); // 1 (old.ts) + 1 (the rename line itself)
        assert_eq!(new_file.churn, 8); // 6 (old.ts) + 2 (rename line)
        assert_eq!(
            result
                .stats
                .alias_to_canonical
                .get("src/old.ts")
                .map(String::as_str),
            Some("src/new.ts")
        );
    }

    #[test]
    fn brace_rename_syntax_is_parsed_and_slashes_are_collapsed() {
        let log = format!(
            "{}\n1\t1\tsrc/{{old => new}}/file.ts\n",
            header("sha1", "2026-01-01T00:00:00Z", "a@x.com", "rename dir")
        );
        let result = parse_git_log(&log, &opts(), FAR_FUTURE_NOW_MS);
        assert!(result.stats.by_path.contains_key("src/new/file.ts"));
        assert_eq!(
            result
                .stats
                .alias_to_canonical
                .get("src/old/file.ts")
                .map(String::as_str),
            Some("src/new/file.ts")
        );
    }

    #[test]
    fn transitive_rename_chain_keeps_both_aliases_pointing_at_final_canonical_path() {
        let log = format!(
            "{}\n1\t0\tsrc/a.ts\n{}\n1\t0\tsrc/a.ts => src/b.ts\n{}\n1\t0\tsrc/b.ts => src/c.ts\n",
            header("sha1", "2026-01-01T00:00:00Z", "a@x.com", "init"),
            header("sha2", "2026-01-02T00:00:00Z", "a@x.com", "rename 1"),
            header("sha3", "2026-01-03T00:00:00Z", "a@x.com", "rename 2")
        );
        let result = parse_git_log(&log, &opts(), FAR_FUTURE_NOW_MS);
        assert!(!result.stats.by_path.contains_key("src/a.ts"));
        assert!(!result.stats.by_path.contains_key("src/b.ts"));
        let c = result.stats.by_path.get("src/c.ts").unwrap();
        assert_eq!(c.change_count, 3);
        assert_eq!(
            result
                .stats
                .alias_to_canonical
                .get("src/a.ts")
                .map(String::as_str),
            Some("src/c.ts")
        );
        assert_eq!(
            result
                .stats
                .alias_to_canonical
                .get("src/b.ts")
                .map(String::as_str),
            Some("src/c.ts")
        );
    }

    #[test]
    fn binary_file_numstat_line_is_excluded_from_stats_and_commit_file_set() {
        let log = format!(
            "{}\n-\t-\tassets/logo.png\n3\t1\tsrc/a.ts\n",
            header("sha1", "2026-01-01T00:00:00Z", "a@x.com", "add asset")
        );
        let result = parse_git_log(&log, &opts(), FAR_FUTURE_NOW_MS);
        assert!(!result.stats.by_path.contains_key("assets/logo.png"));
        assert!(result.stats.by_path.contains_key("src/a.ts"));
        assert_eq!(result.commits[0].files, vec!["src/a.ts".to_string()]);
    }

    #[test]
    fn commit_touching_only_a_binary_file_produces_no_commit_entry() {
        let log = format!(
            "{}\n-\t-\tassets/logo.png\n",
            header("sha1", "2026-01-01T00:00:00Z", "a@x.com", "add asset")
        );
        let result = parse_git_log(&log, &opts(), FAR_FUTURE_NOW_MS);
        assert!(result.commits.is_empty());
        assert!(result.stats.by_path.is_empty());
    }

    #[test]
    fn tag_counts_are_aggregated_per_file_including_revert_before_fix_ordering() {
        let log = format!(
            "{}\n1\t1\tsrc/a.ts\n{}\n1\t1\tsrc/a.ts\n",
            header("sha1", "2026-01-01T00:00:00Z", "a@x.com", "[FIX] bug"),
            header(
                "sha2",
                "2026-01-02T00:00:00Z",
                "a@x.com",
                "Revert \"fix: missing mousewheel event\""
            )
        );
        // The second commit's subject has no `[TAG]` bracket, so classifying it REVERT (rather than
        // FIX, from the quoted text) exercises the keyword table, not just bracket extraction —
        // `opts()`'s empty default table would never classify it, so this test needs a real table (see
        // `tags::test_commit_type_patterns`'s doc for why it's a local copy, not `zzop-metrics`'s).
        let mut o = opts();
        o.commit_type_patterns = tags::test_commit_type_patterns();
        let result = parse_git_log(&log, &o, FAR_FUTURE_NOW_MS);
        let a = result.stats.by_path.get("src/a.ts").unwrap();
        assert_eq!(a.tag_counts.get("FIX").copied(), Some(1));
        assert_eq!(a.tag_counts.get("REVERT").copied(), Some(1));
        assert_eq!(result.commits[0].tags, vec!["FIX".to_string()]);
        assert_eq!(result.commits[1].tags, vec!["REVERT".to_string()]);
    }

    #[test]
    fn recent_window_boundary_excludes_commits_older_than_recent_days() {
        // now = 2026-03-01T00:00:00Z; recent_days = 30 -> cutoff ~ 2026-01-30T00:00:00Z.
        let now_ms = parse_iso_ms_for_test("2026-03-01T00:00:00Z");
        let log = format!(
            "{}\n5\t0\tsrc/a.ts\n{}\n2\t0\tsrc/a.ts\n",
            header("old", "2025-01-01T00:00:00Z", "a@x.com", "old change"),
            header("new", "2026-02-25T00:00:00Z", "a@x.com", "recent change")
        );
        let mut o = opts();
        o.recent_days = 30;
        let result = parse_git_log(&log, &o, now_ms);
        let a = result.stats.by_path.get("src/a.ts").unwrap();
        assert_eq!(a.change_count, 2);
        assert_eq!(a.recent_change_count, Some(1));
        assert_eq!(a.recent_churn, Some(2));
        assert_eq!(
            a.recent_author_commits
                .as_ref()
                .unwrap()
                .get("a@x.com")
                .copied(),
            Some(1)
        );
    }

    #[test]
    fn git_window_reports_since_and_first_last_commit_dates() {
        let log = format!(
            "{}\n1\t0\tsrc/a.ts\n{}\n1\t0\tsrc/b.ts\n",
            header("sha1", "2026-01-01T00:00:00Z", "a@x.com", "one"),
            header("sha2", "2026-06-01T00:00:00Z", "a@x.com", "two")
        );
        let mut o = opts();
        o.since = Some("1.year".to_string());
        let result = parse_git_log(&log, &o, FAR_FUTURE_NOW_MS);
        assert_eq!(result.window.since.as_deref(), Some("1.year"));
        assert_eq!(result.window.first.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(result.window.last.as_deref(), Some("2026-06-01T00:00:00Z"));
        assert_eq!(result.window.commits, 2);
    }

    #[test]
    fn empty_output_produces_empty_collection() {
        let result = parse_git_log("", &opts(), FAR_FUTURE_NOW_MS);
        assert!(result.stats.by_path.is_empty());
        assert!(result.commits.is_empty());
        assert_eq!(result.window.commits, 0);
        assert_eq!(result.window.first, None);
    }

    #[test]
    fn custom_commit_type_patterns_override_the_default_table() {
        let log = format!(
            "{}\n1\t0\tsrc/a.ts\n",
            header("sha1", "2026-01-01T00:00:00Z", "a@x.com", "corrige le bug")
        );
        let mut o = opts();
        o.commit_type_patterns = vec![(r"^\s*corrige\b".to_string(), "FIX".to_string())];
        let result = parse_git_log(&log, &o, FAR_FUTURE_NOW_MS);
        assert_eq!(result.commits[0].tags, vec!["FIX".to_string()]);
    }

    fn parse_iso_ms_for_test(s: &str) -> i64 {
        crate::iso_date::parse_iso_to_ms(s).unwrap()
    }

    // ---------------------------------------------------------------------------------------
    // Integration: a real temp git repo end-to-end through `collect()`.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn collect_end_to_end_against_a_real_temp_git_repo() {
        use std::process::Command;

        let git_available = Command::new("git").arg("--version").output().is_ok();
        if !git_available {
            eprintln!("skipping collect_end_to_end_against_a_real_temp_git_repo: git not on PATH");
            return;
        }

        let dir =
            std::env::temp_dir().join(format!("zzop-git-test-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&dir).expect("create temp repo dir");

        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test User"]);

        std::fs::write(dir.join("a.ts"), "export const a = 1;\n").unwrap();
        run(&["add", "a.ts"]);
        run(&["commit", "-q", "-m", "[FEAT] add a"]);

        std::fs::write(
            dir.join("a.ts"),
            "export const a = 2;\nexport const b = 3;\n",
        )
        .unwrap();
        run(&["add", "a.ts"]);
        run(&["commit", "-q", "-m", "[FIX] correct a"]);

        run(&["mv", "a.ts", "renamed.ts"]);
        run(&["commit", "-q", "-m", "rename a to renamed"]);

        let result = collect(&dir, &CollectOptions::default());
        let collection = result.unwrap_or_else(|e| panic!("collect() failed: {e}"));

        assert_eq!(collection.commits.len(), 3);
        assert!(!collection.stats.by_path.contains_key("a.ts"));
        let renamed = collection
            .stats
            .by_path
            .get("renamed.ts")
            .expect("renamed.ts present as the canonical path");
        assert_eq!(renamed.change_count, 3); // 2 content commits + the rename's numstat line
        assert_eq!(
            collection
                .stats
                .alias_to_canonical
                .get("a.ts")
                .map(String::as_str),
            Some("renamed.ts")
        );
        assert!(renamed.tag_counts.get("FEAT").copied().unwrap_or(0) >= 1);
        assert!(renamed.tag_counts.get("FIX").copied().unwrap_or(0) >= 1);

        let hash = head_hash(&dir).unwrap_or_else(|e| panic!("head_hash() failed: {e}"));
        assert_eq!(hash.len(), 40, "HEAD hash should be a 40-char sha1: {hash}");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Regression test for the non-ASCII path corruption bug: without `-c core.quotepath=false`, git
    /// octal-escapes and double-quotes any path with non-ASCII bytes (e.g. a Korean filename comes back
    /// as `"\355\225\234..."` instead of the real UTF-8 name) in `--numstat` output, so the corrupted
    /// string becomes the `by_path` key instead of the real path — silently dropping that file's churn
    /// from every downstream consumer
    /// that looks it up by its real (disk / dep-graph) path. Exercised as a real temp-repo integration
    /// test (not just a parse-layer fixture) because the bug is specifically in what `process::spawn_git`
    /// passes to the git binary, not in how `parse_git_log` reads its output — a canned-string fixture
    /// would only prove the parser handles UTF-8 correctly, not that the process invocation asks git for
    /// unescaped UTF-8 in the first place.
    #[test]
    fn collect_end_to_end_round_trips_a_non_ascii_korean_filename_unescaped() {
        use std::process::Command;

        let git_available = Command::new("git").arg("--version").output().is_ok();
        if !git_available {
            eprintln!(
                "skipping collect_end_to_end_round_trips_a_non_ascii_korean_filename_unescaped: git not on PATH"
            );
            return;
        }

        let dir = std::env::temp_dir().join(format!(
            "zzop-git-korean-test-{}-{}",
            std::process::id(),
            now_ms()
        ));
        std::fs::create_dir_all(&dir).expect("create temp repo dir");

        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test User"]);
        run(&["config", "core.quotepath", "true"]); // git's own default — the fix must override it per-call

        // A Korean filename, written as \u{..} escapes rather than literal Hangul so this OSS source
        // file stays within the English-only source guard (`scripts/check-english-source.sh`) while
        // still exercising a real non-ASCII (multi-byte UTF-8) path end to end.
        let korean_name = "\u{d55c}\u{ae00}\u{d30c}\u{c77c}.ts";
        std::fs::write(dir.join(korean_name), "export const a = 1;\n").unwrap();
        run(&["add", korean_name]);
        run(&["commit", "-q", "-m", "[FEAT] add korean file"]);

        let result = collect(&dir, &CollectOptions::default());
        let collection = result.unwrap_or_else(|e| panic!("collect() failed: {e}"));

        assert_eq!(collection.commits.len(), 1);
        assert_eq!(collection.commits[0].files, vec![korean_name.to_string()]);
        let stats = collection
            .stats
            .by_path
            .get(korean_name)
            .unwrap_or_else(|| {
                panic!(
                    "expected unescaped key {korean_name:?} in by_path, got keys: {:?}",
                    collection.stats.by_path.keys().collect::<Vec<_>>()
                )
            });
        assert_eq!(stats.change_count, 1);
        // No octal-escaped/quoted phantom key should exist alongside the real one.
        assert!(
            !collection
                .stats
                .by_path
                .keys()
                .any(|k| k.starts_with('"') || k.contains("\\355")),
            "found a quoted/escaped phantom path key: {:?}",
            collection.stats.by_path.keys().collect::<Vec<_>>()
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn collect_on_a_non_git_directory_returns_a_typed_error() {
        let dir = std::env::temp_dir().join(format!("zzop-git-not-a-repo-{}", now_ms()));
        std::fs::create_dir_all(&dir).expect("create plain temp dir");
        let result = collect(&dir, &CollectOptions::default());
        assert!(matches!(result, Err(GitError::NotAGitRepository { .. })));
        let hash_result = head_hash(&dir);
        assert!(matches!(
            hash_result,
            Err(GitError::NotAGitRepository { .. })
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn collect_on_a_missing_path_returns_a_typed_error_without_panicking() {
        let dir = std::env::temp_dir().join(format!("zzop-git-missing-{}", now_ms()));
        let result = collect(&dir, &CollectOptions::default());
        assert!(matches!(result, Err(GitError::NotAGitRepository { .. })));
    }
}
