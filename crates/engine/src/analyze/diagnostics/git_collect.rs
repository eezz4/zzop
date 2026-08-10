//! Git collection + custom commit-type-pattern validation — see `collect_git`'s doc.

use zzop_core::GitStats;

use crate::EngineConfig;

mod cache;

#[cfg(test)]
mod tests;

pub(crate) use cache::GitCache;

/// Runs `zzop_git::collect` when `config.git` is `Some`, pushing a warning (never panicking, never
/// failing the analysis) when `root` is not a git repository / `git` is unavailable / collection
/// otherwise fails. Returns `(GitStats::default(), vec![], false)` for every "not active" case so the
/// caller's git-dependent computations can gate on the returned `bool` alone.
///
/// `git_cache` collapses repeated collections across the trees of one run — see [`GitCache`]. The
/// warnings this function pushes are still derived PER TREE from the (possibly shared) collection, so
/// a memo hit produces exactly the text a fresh collection would have.
pub(in crate::analyze) fn collect_git(
    root: &std::path::Path,
    config: &EngineConfig,
    warnings: &mut Vec<String>,
    git_cache: &GitCache,
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
    // A tree with no resolvable `.git` ancestor gets no memo identity — it collects on its own (and
    // will fail on its own, producing the same warning it always did).
    let repo_root = zzop_git::repo_root(root).unwrap_or_else(|| root.to_path_buf());
    match git_cache.get_or_collect(&repo_root, root, &opts) {
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
