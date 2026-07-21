//! Git collection + custom commit-type-pattern validation — see `collect_git`'s doc.

use zzop_core::GitStats;

use crate::EngineConfig;

/// Runs `zzop_git::collect` when `config.git` is `Some`, pushing a warning (never panicking, never
/// failing the analysis) when `root` is not a git repository / `git` is unavailable / collection
/// otherwise fails. Returns `(GitStats::default(), vec![], false)` for every "not active" case so the
/// caller's git-dependent computations can gate on the returned `bool` alone.
pub(in crate::analyze) fn collect_git(
    root: &std::path::Path,
    config: &EngineConfig,
    warnings: &mut Vec<String>,
) -> (GitStats, Vec<zzop_core::CommitFileSet>, bool) {
    let Some(git_opts) = &config.git else {
        return (GitStats::default(), Vec::new(), false);
    };
    // The default FIX/FEAT/... keyword vocabulary is analysis-domain, not collection-mechanism, so it
    // lives in `zzop-metrics` rather than `zzop-git` — collector crates own the mechanism, not the domain
    // vocabulary. A config `git.commitTypePatterns` table (`GitOptions::commit_type_patterns`) REPLACES
    // that default table whole when present and non-empty; empty/absent falls back to the default.
    let commit_type_patterns = match &git_opts.commit_type_patterns {
        Some(custom) if !custom.is_empty() => {
            warn_on_invalid_commit_type_patterns(custom, warnings);
            custom.clone()
        }
        _ => zzop_metrics::default_commit_type_patterns(),
    };
    let opts = zzop_git::CollectOptions {
        since: git_opts.since.clone(),
        recent_days: git_opts.recent_days,
        commit_type_patterns,
    };
    match zzop_git::collect(root, &opts) {
        Ok(collection) => (collection.stats, collection.commits, true),
        Err(e) => {
            warnings.push(format!(
                "git collection skipped for {}: {e}",
                root.display()
            ));
            (GitStats::default(), Vec::new(), false)
        }
    }
}

/// Validates a custom `git.commitTypePatterns` table before it reaches `zzop_git`: `zzop_git`'s own
/// compile step (`zzop_git::tags::CommitClassifiers::compile`) silently DROPS a pattern that fails to
/// compile as a regex — never panics, but never tells the caller either. A user-supplied pattern is
/// exactly the kind of narrowed-scope degradation this codebase's "self-reports in warnings, never
/// silently" contract exists for (mirrors `unmatched_suppression_warnings`'s "the filter had no effect"
/// self-report for a different config knob), so this pushes one warning naming every pattern that fails to
/// compile. The custom table is still passed to `zzop_git` unfiltered either way — an invalid pattern is
/// simply inert there too (matches nothing), exactly as `zzop_git` already treats it; this only makes that
/// outcome visible instead of silent.
///
/// Compiles via [`zzop_git::compile_commit_type_pattern`] — the SAME function `CommitClassifiers::compile`
/// calls to actually use the pattern — rather than re-deriving the `(?i)` prefix locally, so this
/// validator's verdict and `zzop_git`'s real behavior cannot drift (see `compile_coupling_tests` below).
fn warn_on_invalid_commit_type_patterns(patterns: &[(String, String)], warnings: &mut Vec<String>) {
    let bad: Vec<&str> = patterns
        .iter()
        .filter(|(pattern, _)| zzop_git::compile_commit_type_pattern(pattern).is_err())
        .map(|(pattern, _)| pattern.as_str())
        .collect();
    if bad.is_empty() {
        return;
    }
    warnings.push(format!(
        "git.commitTypePatterns has {} invalid regex pattern(s), skipped (matches nothing): {} — check for unescaped regex metacharacters.",
        bad.len(),
        bad.join(", ")
    ));
}

/// Twin-compile-path pin (see `warn_on_invalid_commit_type_patterns`'s own doc). Historically this
/// validator and `zzop_git::tags::CommitClassifiers::compile` (`crates/git/src/tags.rs`) each
/// independently compiled a caller-supplied `git.commitTypePatterns` regex with the SAME `(?i)` prefix —
/// one to DETECT invalidity (here), one to USE it (deep inside `zzop_git`, behind a
/// `pub(crate)`-to-that-crate type this crate cannot name directly) — and nothing cross-checked that the
/// two verdicts actually agreed, so a prefix/flag change on either side alone could silently desync the
/// validator's "skipped, matches nothing" warning from what `zzop_git` really did with the same pattern.
///
/// Both sides now call the single shared [`zzop_git::compile_commit_type_pattern`], so that class of
/// drift is no longer possible by construction — this test is kept anyway as an end-to-end behavior lock
/// (not a mechanism check): it drives real `zzop_git` tag classification through
/// [`zzop_git::parse_git_log`] (its only PUBLIC pure entry point) against one canned commit whose entire
/// tag classification comes from the single caller-supplied pattern under test, proving the validator's
/// verdict matches what actually gets tagged — not merely that the two call sites happen to name the same
/// function. Modeled on the twin-duplicate pin idiom in `crate::dead_exports`'s
/// `call_graph_covered_extensions_pin`.
#[cfg(test)]
mod compile_coupling_tests {
    use super::warn_on_invalid_commit_type_patterns;

    const SEP: char = '\u{1f}';

    /// One `git log --numstat`-shaped commit (the crate's documented wire format — see
    /// `zzop_git::parse_git_log`'s own doc): `__C__<sha><SEP><isoDate><SEP><author><SEP><subject>`
    /// header, one numstat line. `zzop_git`'s own marker/separator constants are `pub(crate)` to that
    /// crate (unreachable here), so this literal-embeds the same documented values instead of importing
    /// them.
    fn canned_log(subject: &str) -> String {
        format!("__C__sha1{SEP}2026-01-01T00:00:00Z{SEP}a@x.com{SEP}{subject}\n1\t0\tf.ts\n")
    }

    /// The tags `zzop_git` actually assigns the canned commit when `pattern` (paired with `tag`) is the
    /// ONLY entry in its classifier table — empty when `zzop_git` treated the pattern as inert (either
    /// it failed to compile there, or it compiled but did not match `subject`).
    fn tags_from_zzop_git(pattern: &str, tag: &str, subject: &str) -> Vec<String> {
        let opts = zzop_git::CollectOptions {
            commit_type_patterns: vec![(pattern.to_string(), tag.to_string())],
            ..zzop_git::CollectOptions::default()
        };
        zzop_git::parse_git_log(&canned_log(subject), &opts, 0)
            .commits
            .into_iter()
            .next()
            .map(|c| c.tags)
            .unwrap_or_default()
    }

    #[test]
    fn a_valid_pattern_is_judged_valid_by_the_validator_and_actually_tags_in_zzop_git() {
        let pattern = r"^\s*flimflam\b";
        let mut warnings = Vec::new();
        warn_on_invalid_commit_type_patterns(
            &[(pattern.to_string(), "FLIM".to_string())],
            &mut warnings,
        );
        assert!(
            warnings.is_empty(),
            "validator must judge a well-formed pattern valid, got: {warnings:?}"
        );
        let tags = tags_from_zzop_git(pattern, "FLIM", "flimflam did a thing");
        assert_eq!(
            tags,
            vec!["FLIM".to_string()],
            "zzop_git must actually compile and apply the same pattern the validator accepted \
             (both use the same `(?i)` prefix) — a mismatch here means the two compile paths have \
             diverged"
        );
    }

    #[test]
    fn an_invalid_pattern_is_judged_invalid_by_the_validator_and_never_tags_in_zzop_git() {
        // An unclosed character class — fails to compile as a regex (same pattern
        // `crates/engine/tests/analyze_git.rs`'s end-to-end warning-path test uses).
        let pattern = "[unclosed";
        let mut warnings = Vec::new();
        warn_on_invalid_commit_type_patterns(
            &[(pattern.to_string(), "FIX".to_string())],
            &mut warnings,
        );
        assert!(
            warnings.iter().any(|w| w.contains(pattern)),
            "validator must judge the malformed pattern invalid, got: {warnings:?}"
        );
        // No subject can ever be tagged by a pattern that fails to compile in `zzop_git` either (its
        // classifier table is simply empty for this entry) — a subject otherwise unclassifiable by
        // anything else proves the point without assuming what `[unclosed` would have matched had it
        // compiled.
        let tags = tags_from_zzop_git(pattern, "FIX", "totally unrelated subject text");
        assert!(
            tags.is_empty(),
            "zzop_git must also treat the malformed pattern as inert (empty compiled classifier \
             list), got: {tags:?} — a mismatch here means the validator rejects patterns zzop_git \
             actually still accepts (or vice versa)"
        );
    }
}
