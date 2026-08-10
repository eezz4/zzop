//! Test-only pins for `git_collect` — the twin-compile-path pin and the lossy-subject disclosure
//! gate. Split out of `git_collect.rs` on 2026-08-08 (the run-scoped git memo pushed that file over
//! the line ratchet); both modules keep their `super::` paths, which still resolve from here.
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
    use super::super::warn_on_invalid_commit_type_patterns;

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
    use super::super::warn_on_inert_commit_subject_patterns;

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
