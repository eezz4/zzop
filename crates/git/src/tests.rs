//! `parse_git_log` tests against canned `git log --numstat` text using this crate's format (see
//! `process::run_git_log`): `__C__<sha>\x1f<isoDate>\x1f<author>\x1f<subject>` header lines,
//! numstat lines (`added\tdeleted\tpath`, renames as `old => new` / `{old => new}`, binary as
//! `-\t-\tpath`). These cases cover `parse_git_log`'s documented semantics: rename merging, binary
//! exclusion, multi-author aggregation, tag classification (incl. REVERT-before-FIX), and the
//! recent-window boundary; the tag-classifier cases specifically live in `tags.rs`.
use super::*;
use zzop_test_support::skip_notice;

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
        skip_notice!("git not on PATH");
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
        skip_notice!("git not on PATH");
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

/// Regression test for the cwd-sensitive numstat bug: with `diff.relative=true` in effect (a plain
/// user-level config), `git log --numstat` emits paths relative to the process cwd AND silently
/// DROPS files outside the cwd. `collect()` runs in the caller's tree root, which in a monorepo is a
/// SUBDIRECTORY of the repository — and the engine memoizes the collection per repo root, so one
/// poisoned collection would be shared with every other tree of the run. `process::spawn_git` must
/// pin `-c diff.relative=false` per call, same choke point as `core.quotepath` above.
#[test]
fn collect_from_a_subdirectory_is_immune_to_diff_relative_config() {
    use std::process::Command;

    let git_available = Command::new("git").arg("--version").output().is_ok();
    if !git_available {
        skip_notice!("git not on PATH");
        return;
    }

    let dir = std::env::temp_dir().join(format!(
        "zzop-git-diff-relative-test-{}-{}",
        std::process::id(),
        now_ms()
    ));
    std::fs::create_dir_all(dir.join("api")).expect("create temp repo dirs");
    std::fs::create_dir_all(dir.join("web")).expect("create temp repo dirs");

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
    // The hostile setting, repo-local so the test needs no global state — the fix must override it
    // per call exactly like `core.quotepath` above.
    run(&["config", "diff.relative", "true"]);

    std::fs::write(dir.join("api/main.ts"), "export const a = 1;\n").unwrap();
    std::fs::write(dir.join("web/app.ts"), "export const b = 2;\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "[FEAT] add both trees"]);

    // Collect from the SUBDIRECTORY, as the engine does for a monorepo tree root.
    let result = collect(&dir.join("api"), &CollectOptions::default());
    let collection = result.unwrap_or_else(|e| panic!("collect() failed: {e}"));

    // Repo-root-relative and complete: the sibling tree's file must not be dropped, and the cwd
    // tree's file must keep its full path (not a cwd-relative "main.ts").
    for key in ["api/main.ts", "web/app.ts"] {
        assert!(
            collection.stats.by_path.contains_key(key),
            "expected repo-root-relative key {key:?} in by_path, got keys: {:?}",
            collection.stats.by_path.keys().collect::<Vec<_>>()
        );
    }
    assert!(
        !collection.stats.by_path.contains_key("main.ts"),
        "found a cwd-relative phantom key: {:?}",
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
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn collect_on_a_missing_path_returns_a_typed_error_without_panicking() {
    let dir = std::env::temp_dir().join(format!("zzop-git-missing-{}", now_ms()));
    let result = collect(&dir, &CollectOptions::default());
    assert!(matches!(result, Err(GitError::NotAGitRepository { .. })));
}

// --- commit subject preservation + declared-pattern labels -----------------------------------------

/// Seals that the parse carries the subject WHOLE and unmodified, including the punctuation/quoting that
/// a derived field would have normalized away — `tags`/`labels` are lossy views, this is not. Scope: this
/// starts from a `&str`, i.e. DOWNSTREAM of the decode boundary; what happens to non-UTF-8 bytes before
/// that is pinned by `decode_boundary_tests` below.
#[test]
fn the_commit_subject_is_carried_whole_and_unmodified() {
    let subject = "Revert \"feat(api): add /v2 users\" — see PROJ-42";
    let log = format!(
        "{}\n1\t0\tsrc/a.ts\n",
        header("sha1", "2026-01-01T00:00:00Z", "a@x.com", subject)
    );
    let result = parse_git_log(&log, &opts(), FAR_FUTURE_NOW_MS);
    assert_eq!(result.commits[0].subject.as_deref(), Some(subject));
}

/// Seals that subject preservation and `[TAG]` extraction BOTH happen on the same commit — the two are
/// independent readings of one subject, so adding the raw field must not consume or alter the tag path
/// (and the tag path must not truncate the raw field to the untagged remainder).
#[test]
fn a_bracket_tagged_subject_keeps_both_its_tags_and_its_raw_text() {
    let subject = "[FIX][PERF] tighten the cache key";
    let log = format!(
        "{}\n1\t0\tsrc/a.ts\n",
        header("sha1", "2026-01-01T00:00:00Z", "a@x.com", subject)
    );
    let result = parse_git_log(&log, &opts(), FAR_FUTURE_NOW_MS);
    let c = &result.commits[0];
    assert_eq!(c.tags, vec!["FIX".to_string(), "PERF".to_string()]);
    assert_eq!(c.subject.as_deref(), Some(subject));
}

/// Seals never-guess end to end through the parser: with NO declared subject patterns, subjects that any
/// plausible built-in convention would have labelled get zero labels — the subject is preserved, and
/// nothing is inferred from it.
#[test]
fn without_a_declaration_no_commit_is_labelled() {
    let log = format!(
        "{}\n1\t0\tsrc/a.ts\n{}\n1\t0\tsrc/b.ts\n",
        header(
            "sha1",
            "2026-01-01T00:00:00Z",
            "a@x.com",
            "Revert \"add caching\""
        ),
        header(
            "sha2",
            "2026-01-02T00:00:00Z",
            "a@x.com",
            "hotfix: PROJ-42 null deref"
        )
    );
    let result = parse_git_log(&log, &opts(), FAR_FUTURE_NOW_MS);
    assert!(
        result.commits.iter().all(|c| c.labels.is_empty()),
        "an undeclared axis must classify nothing, got: {:?}",
        result.commits.iter().map(|c| &c.labels).collect::<Vec<_>>()
    );
    assert!(result.commits.iter().all(|c| c.subject.is_some()));
}

/// Seals that a DECLARED table really labels, that several declarations can apply to one subject, and
/// that the order is the table's declaration order (not match position) — the determinism the labels
/// riding in any output depends on.
#[test]
fn declared_subject_patterns_label_in_declaration_order() {
    let o = CollectOptions {
        commit_subject_patterns: vec![
            (r"PROJ-\d+".to_string(), "ticket".to_string()),
            (r"^Revert\b".to_string(), "revert".to_string()),
        ],
        ..CollectOptions::default()
    };
    let log = format!(
        "{}\n1\t0\tsrc/a.ts\n{}\n1\t0\tsrc/b.ts\n",
        header(
            "sha1",
            "2026-01-01T00:00:00Z",
            "a@x.com",
            "Revert \"PROJ-42 add caching\""
        ),
        header("sha2", "2026-01-02T00:00:00Z", "a@x.com", "tidy imports")
    );
    let result = parse_git_log(&log, &o, FAR_FUTURE_NOW_MS);
    assert_eq!(
        result.commits[0].labels,
        vec!["ticket".to_string(), "revert".to_string()]
    );
    assert!(result.commits[1].labels.is_empty());
}

/// Seals that the declared-label axis and the commit-TYPE axis are independent: a `[TAG]` bracket does
/// not suppress labels (it DOES suppress the keyword classifier — that asymmetry is deliberate), and a
/// declared label never lands in `tags`.
#[test]
fn declared_labels_and_bracket_tags_are_independent_axes() {
    let o = CollectOptions {
        commit_subject_patterns: vec![(r"PROJ-\d+".to_string(), "ticket".to_string())],
        ..CollectOptions::default()
    };
    let log = format!(
        "{}\n1\t0\tsrc/a.ts\n",
        header(
            "sha1",
            "2026-01-01T00:00:00Z",
            "a@x.com",
            "[FIX] PROJ-42 null deref"
        )
    );
    let result = parse_git_log(&log, &o, FAR_FUTURE_NOW_MS);
    let c = &result.commits[0];
    assert_eq!(c.tags, vec!["FIX".to_string()]);
    assert_eq!(c.labels, vec!["ticket".to_string()]);
}

/// Seals that an empty subject stays `None` rather than becoming `Some("")` — "this commit has no
/// subject" and "the subject is the empty string" must not be told apart by accident downstream.
#[test]
fn an_empty_subject_is_none_not_an_empty_string() {
    let log = format!(
        "{}\n1\t0\tsrc/a.ts\n",
        header("sha1", "2026-01-01T00:00:00Z", "a@x.com", "")
    );
    let result = parse_git_log(&log, &opts(), FAR_FUTURE_NOW_MS);
    assert_eq!(result.commits[0].subject, None);
}

// --- the decode boundary (bytes -> String), which every test above starts downstream of -------------

/// Every test above hands `parse_git_log` a `&str`, so none of them can see what happens to git's raw
/// BYTES — the step where `process::decode_git_output` turns stdout into that `&str`. These pin the
/// boundary itself: what a non-UTF-8 byte becomes, and what that does to declared-pattern matching.
mod decode_boundary_tests {
    use super::*;

    /// `git log %s` output for a subject written in latin-1 by a commit object with no `encoding`
    /// header: git re-encodes only when that header is present, so the raw `0xE9` reaches us as-is.
    const LATIN1_SUBJECT: &[u8] = b"caf\xe9 legacy subject";

    /// One commit's raw `git log --numstat` stdout, subject spliced in as BYTES (not `&str`), so the
    /// fixture can carry a sequence that is not valid UTF-8 at all.
    fn raw_log_bytes(subject: &[u8]) -> Vec<u8> {
        let mut out = Vec::from(&b"__C__sha1\x1f2026-01-01T00:00:00Z\x1fa@x.com\x1f"[..]);
        out.extend_from_slice(subject);
        out.extend_from_slice(b"\n1\t0\tsrc/a.ts\n");
        out
    }

    /// Seals WHAT the decode boundary does to a non-UTF-8 byte: it is replaced by U+FFFD, never
    /// preserved and never an error (`from_utf8_lossy` is the deliberate never-fail choice).
    #[test]
    fn a_non_utf8_subject_byte_is_replaced_by_u_fffd_not_carried_and_not_an_error() {
        let raw = raw_log_bytes(LATIN1_SUBJECT);
        assert!(
            raw.contains(&0xE9),
            "fixture must actually be invalid UTF-8 before decoding, else this test never crosses \
             the boundary it exists to pin"
        );
        let decoded = crate::process::decode_git_output(&raw);
        assert!(
            !decoded.as_bytes().contains(&0xE9),
            "the raw byte must not survive decoding: {decoded:?}"
        );
        assert!(
            decoded.contains('\u{FFFD}'),
            "the non-UTF-8 byte must become U+FFFD: {decoded:?}"
        );
    }

    /// Seals what the PARSE therefore sees: the preserved subject is the post-replacement text, not
    /// git's bytes — the concrete counterexample to "verbatim, exactly as `git log %s` emitted it".
    #[test]
    fn the_preserved_subject_is_the_lossily_decoded_text_not_gits_bytes() {
        let decoded = crate::process::decode_git_output(&raw_log_bytes(LATIN1_SUBJECT));
        let result = parse_git_log(&decoded, &opts(), FAR_FUTURE_NOW_MS);
        assert_eq!(
            result.commits[0].subject.as_deref(),
            Some("caf\u{FFFD} legacy subject")
        );
    }

    /// Seals the user-visible consequence: a declared pattern spelled with the ORIGINAL non-ASCII text
    /// can never match such a subject (the character it targets no longer exists by match time), while
    /// the ASCII part of the very same subject still labels normally — so the commit is present and
    /// labellable, and only the encoding-dependent pattern is inert.
    #[test]
    fn a_declared_pattern_with_the_original_non_ascii_text_cannot_match_after_lossy_decode() {
        let decoded = crate::process::decode_git_output(&raw_log_bytes(LATIN1_SUBJECT));
        let o = CollectOptions {
            commit_subject_patterns: vec![
                ("café".to_string(), "cafe".to_string()),
                ("legacy".to_string(), "legacy".to_string()),
            ],
            ..CollectOptions::default()
        };
        let result = parse_git_log(&decoded, &o, FAR_FUTURE_NOW_MS);
        assert_eq!(result.commits[0].labels, vec!["legacy".to_string()]);
    }

    /// Seals that the boundary is lossy ONLY for invalid input: a valid-UTF-8 non-ASCII subject
    /// survives it byte for byte, so the replacement above is not the decoder mangling everything.
    #[test]
    fn a_valid_utf8_non_ascii_subject_survives_the_decode_boundary_unchanged() {
        let decoded = crate::process::decode_git_output(&raw_log_bytes("café résumé".as_bytes()));
        let result = parse_git_log(&decoded, &opts(), FAR_FUTURE_NOW_MS);
        assert_eq!(result.commits[0].subject.as_deref(), Some("café résumé"));
    }
}
