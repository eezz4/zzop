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
    // The DECLARED subject-pattern axis, by contrast, has no default table to fall back to and never
    // will: absent/empty stays absent/empty, so a repo whose config says nothing gets no labels rather
    // than labels from a convention nobody declared. This is the whole point of the key.
    let commit_subject_patterns = git_opts.commit_subject_patterns.clone().unwrap_or_default();
    warn_on_invalid_commit_subject_patterns(&commit_subject_patterns, warnings);
    let opts = zzop_git::CollectOptions {
        since: git_opts.since.clone(),
        recent_days: git_opts.recent_days,
        commit_type_patterns,
        commit_subject_patterns: commit_subject_patterns.clone(),
    };
    match zzop_git::collect(root, &opts) {
        Ok(collection) => {
            warn_on_inert_commit_subject_patterns(
                &commit_subject_patterns,
                &collection.commits,
                warnings,
            );
            (collection.stats, collection.commits, true)
        }
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

/// The `git.commitSubjectPatterns` twin of `warn_on_invalid_commit_type_patterns` — same contract, same
/// reason, and deliberately compiled through [`zzop_git::compile_commit_subject_pattern`] rather than
/// `compile_commit_type_pattern`: the two axes do NOT share compile semantics (the subject axis injects
/// no `(?i)`), so validating one with the other's compiler would be the exact drift the shared-function
/// rule exists to prevent, only in a subtler form — a `(?i)`-dependent pattern would pass validation
/// here and then match nothing in `zzop_git`.
fn warn_on_invalid_commit_subject_patterns(
    patterns: &[(String, String)],
    warnings: &mut Vec<String>,
) {
    let bad: Vec<&str> = patterns
        .iter()
        .filter(|(pattern, _)| zzop_git::compile_commit_subject_pattern(pattern).is_err())
        .map(|(pattern, _)| pattern.as_str())
        .collect();
    if bad.is_empty() {
        return;
    }
    warnings.push(format!(
        "git.commitSubjectPatterns has {} invalid regex pattern(s), skipped (matches nothing): {} — check for unescaped regex metacharacters.",
        bad.len(),
        bad.join(", ")
    ));
}

/// A DECLARED subject-pattern table that labelled zero commits self-reports, the same way an unmatched
/// suppression does. This axis is opt-in with no fallback, so "no labels anywhere" is ambiguous from the
/// outside — it reads identically whether the author declared nothing (correct silence) or declared a
/// table whose regexes never fire against this repo's real subjects (a dead knob). Only the second is a
/// problem, and only this layer can tell them apart, so only this layer can say it. One line, naming the
/// declared labels in declaration order (deterministic); never an error, and the analysis is unaffected.
///
/// A second sentence rides along ONLY when a U+FFFD is actually observed (see
/// [`subjects_show_lossy_decoding`]): the bare line leaves the declared pattern as the only suspect, when
/// the cause may be that `zzop_git`'s `String::from_utf8_lossy` decode of git's stdout already replaced
/// the non-UTF-8 bytes the pattern was written against. Phrased as a possibility, never a verdict — a
/// U+FFFD can also be genuinely part of the original subject, and this layer cannot tell the two apart.
fn warn_on_inert_commit_subject_patterns(
    patterns: &[(String, String)],
    commits: &[zzop_core::CommitFileSet],
    warnings: &mut Vec<String>,
) {
    if patterns.is_empty() || commits.iter().any(|c| !c.labels.is_empty()) {
        return;
    }
    let mut labels: Vec<&str> = Vec::new();
    for (_, label) in patterns {
        if !labels.contains(&label.as_str()) {
            labels.push(label.as_str());
        }
    }
    let lossy_hint = if subjects_show_lossy_decoding(commits) {
        " Note: some collected subjects contain U+FFFD (the Unicode replacement character), which is what a non-UTF-8 byte becomes when git's output is decoded — subjects from legacy-encoded history may have lost characters before matching, so a non-ASCII pattern can be unable to match text that looks correct in `git log`. (U+FFFD may also be genuinely part of the original subject.)"
    } else {
        ""
    };
    warnings.push(format!(
        "git.commitSubjectPatterns declared {} pattern(s) ({}) but none matched any of the {} collected commit subject(s) — nothing was labelled.{lossy_hint}",
        patterns.len(),
        labels.join(", "),
        commits.len()
    ));
}

/// Whether any collected subject carries the Unicode replacement character — the only evidence this layer
/// has that collection's `from_utf8_lossy` may have dropped bytes. Observation, not inference.
fn subjects_show_lossy_decoding(commits: &[zzop_core::CommitFileSet]) -> bool {
    commits
        .iter()
        .filter_map(|c| c.subject.as_deref())
        .any(|s| s.contains('\u{FFFD}'))
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

/// Evidence gate on the inert warning's encoding sentence (see `warn_on_inert_commit_subject_patterns`):
/// disclosed only when a U+FFFD is actually observed, never guessed at from a bare zero-match outcome.
#[cfg(test)]
mod lossy_subject_disclosure_tests {
    use super::warn_on_inert_commit_subject_patterns;

    /// The single inert warning produced for one collected commit carrying `subject`.
    fn inert_warning_for(subject: &str) -> String {
        let commits = vec![zzop_core::CommitFileSet {
            sha: "sha1".to_string(),
            files: vec!["a.ts".to_string()],
            tags: Vec::new(),
            date: None,
            subject: Some(subject.to_string()),
            labels: Vec::new(),
        }];
        let mut warnings = Vec::new();
        warn_on_inert_commit_subject_patterns(
            &[("café".to_string(), "cafe".to_string())],
            &commits,
            &mut warnings,
        );
        warnings.remove(0)
    }

    /// Seals that an observed U+FFFD is disclosed, so a zero-match report points at the encoding as a
    /// possible cause instead of leaving the declared pattern as the only suspect.
    #[test]
    fn an_observed_replacement_character_is_disclosed_in_the_inert_warning() {
        let w = inert_warning_for("caf\u{FFFD} legacy subject");
        assert!(w.contains("U+FFFD"), "{w}");
    }

    /// Seals never-guess: with no U+FFFD anywhere in the collected subjects, the warning stays exactly
    /// the plain zero-match line — no speculation about encodings that were never observed.
    #[test]
    fn a_clean_subject_produces_the_plain_warning_with_no_encoding_speculation() {
        let w = inert_warning_for("perfectly ordinary subject");
        assert!(!w.contains("U+FFFD"), "{w}");
    }
}
