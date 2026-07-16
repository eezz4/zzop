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
fn warn_on_invalid_commit_type_patterns(patterns: &[(String, String)], warnings: &mut Vec<String>) {
    let bad: Vec<&str> = patterns
        .iter()
        .filter(|(pattern, _)| regex::Regex::new(&format!("(?i){pattern}")).is_err())
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
