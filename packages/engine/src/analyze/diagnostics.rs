//! Capability self-reports / warnings / git collection — the honest-degrade surface `assemble` folds
//! into `AnalyzeOutput::warnings`: git collection (or its absence), the coverage-gap diagnostics report,
//! zero-DSL-packs and minified-file-skip self-reports, and unknown-disabled-rule-id detection.

use zzop_core::{DepGraph, GitStats};
use zzop_metrics::{build_diagnostics, DiagnosticsInput, GitDiagnosticsInput};

use crate::EngineConfig;

/// Runs `zzop_git::collect` when `config.git` is `Some`, pushing a warning (never panicking, never
/// failing the analysis) when `root` is not a git repository / `git` is unavailable / collection
/// otherwise fails. Returns `(GitStats::default(), vec![], false)` for every "not active" case so the
/// caller's git-dependent computations can gate on the returned `bool` alone.
pub(super) fn collect_git(
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

/// Builds `zzop_metrics::diagnostics`' coverage-gap self-report from data `assemble` already has in
/// scope — no extra pass. `symbols` filters on `SourceSymbol::exported` since `all_symbols` also
/// carries unexported top-level declarations. `concrete_modules`/`total_modules` are always `0` — no
/// real module classification is wired at this call site yet, and `0`/`0` is the honest "not measured"
/// value (the module's own `total_modules > 1` guard means that pair simply never fires until it is).
///
/// **Git-disabled gating**: `DiagnosticsInput::git` is `Option<GitDiagnosticsInput>` so the module
/// itself can tell "git was never attempted" (`None`) apart from "git ran and found zero" (`Some` with
/// honest zero counts) — `build_diagnostics` skips every git-window warning when `git` is `None`. This
/// passes `None` when `git_active` is `false`, `Some` with the honest counts otherwise.
pub(crate) fn run_diagnostics(
    file_count: usize,
    dep: &DepGraph,
    symbols: &[zzop_core::SourceSymbol],
    commits: &[zzop_core::CommitFileSet],
    config: &EngineConfig,
    git_active: bool,
) -> Vec<String> {
    let dep_edges: u32 = dep.values().map(|targets| targets.len() as u32).sum();
    let exported_symbols = symbols.iter().filter(|s| s.exported).count() as u32;

    let git = git_active.then(|| {
        let (total_changes, tagged_changes, fix_changes) =
            commits
                .iter()
                .fold((0u32, 0u32, 0u32), |(total, tagged, fix), c| {
                    let n = c.files.len() as u32;
                    let tagged = tagged + if c.tags.is_empty() { 0 } else { n };
                    let fix = fix
                        + if c.tags.iter().any(|t| t == "FIX") {
                            n
                        } else {
                            0
                        };
                    (total + n, tagged, fix)
                });
        GitDiagnosticsInput {
            total_changes,
            tagged_changes,
            fix_changes,
            commits: commits.len() as u32,
            since: config.git.as_ref().and_then(|g| g.since.clone()),
        }
    });

    let diagnostics = build_diagnostics(DiagnosticsInput {
        files: file_count as u32,
        dep_edges,
        symbols: exported_symbols,
        concrete_modules: 0,
        total_modules: 0,
        git,
        unknown_disabled_rule_ids: unknown_disabled_rule_ids(config),
    });

    diagnostics.warnings
}

/// Capability self-report: git history was never requested (`config.git` is `None`), so every
/// git-derived output channel is null. Distinct from `collect_git`'s own warning, which fires only when
/// git WAS requested but collection failed — a consumer can always tell "never asked" apart from
/// "asked, failed" by which of the two strings is present. Returns `None` when git was requested.
pub(super) fn git_not_requested_warning(config: &EngineConfig) -> Option<String> {
    if config.git.is_some() {
        return None;
    }
    Some(
        "git history not requested (git option omitted): scores, health, recommendations, criticality, seams and layerCoChurn are null. Pass git: {} to enable them."
            .to_string(),
    )
}

/// Capability self-report: no DSL rule packs are loaded (`config.packs` is empty), so only the built-in
/// native analyses ran. `pub(crate)` because it is shared between `assemble` and
/// `envelope::analyze_envelope`, which gate DSL packs identically on `config.packs`. Per this codebase's
/// kernel-agnostic-no-rule-data principle the message names no rule/vocab, only the native-analysis
/// count and the `packsDir` config hint. Returns `None` when at least one pack is loaded.
pub(crate) fn zero_packs_warning(config: &EngineConfig) -> Option<String> {
    if !config.packs.is_empty() {
        return None;
    }
    let mut registry = zzop_core::RuleRegistry::new();
    crate::register_all_native(&mut registry);
    let native_count = registry.metas().len();
    Some(format!(
        "no DSL rule packs loaded: only the {native_count} built-in native analyses ran. If you expected the bundled packs, reinstall/check the package (the bundled packs directory may be missing); to add your own, set `packs: {{ extraDirs: [...] }}` in zzop.config.jsonc (embedders: `packsDir`)."
    ))
}

/// Capability self-report: how many files this run classified minified/generated and were therefore
/// skipped for every DSL rule-pack matcher type (distinct from `degraded`, which still runs line-scan
/// rules). One aggregate entry, never one per file. `sorted_rels` must already be sorted. Returns `None`
/// when nothing was skipped this way.
pub(super) fn minified_files_warning(sorted_rels: &[String]) -> Option<String> {
    if sorted_rels.is_empty() {
        return None;
    }
    const SAMPLE: usize = 3;
    let sample: Vec<&str> = sorted_rels
        .iter()
        .take(SAMPLE)
        .map(String::as_str)
        .collect();
    let mut sample_str = sample.join(", ");
    if sorted_rels.len() > SAMPLE {
        sample_str.push_str(&format!(", +{} more", sorted_rels.len() - SAMPLE));
    }
    Some(format!(
        "{} minified/generated file(s) skipped for ALL DSL rule-pack rules (long-line-dominated or 5000+ byte single lines; native structural analyses still cover them): {sample_str}",
        sorted_rels.len()
    ))
}

/// Capability self-report: a `rules[].exclude` (suppression) whose path/glob filter matches NONE of the
/// scanned files — almost always a typo (classically `*.stories.tsx`, whose `*` cannot cross `/`, missing
/// every nested `src/**/x.stories.tsx`). Mirrors `unknown_disabled_rule_ids`: honest, one warning per dead
/// filter. Whole-rule suppressions (no path/glob) are never flagged (they legitimately match everything).
pub(crate) fn unmatched_suppression_warnings(config: &EngineConfig, rels: &[&str]) -> Vec<String> {
    config
        .rule_config
        .suppressions
        .iter()
        .filter(|entry| entry.glob.is_some() || entry.path.is_some())
        .filter(|entry| {
            !rels
                .iter()
                .any(|rel| zzop_core::suppression_matches_path(entry, rel))
        })
        .map(|entry| {
            if let Some(glob) = &entry.glob {
                let hint = if looks_segment_bound(glob) {
                    format!(
                        " — a leading '*' does not cross '/'; did you mean \"**/{glob}\"?"
                    )
                } else {
                    String::new()
                };
                format!(
                    "exclude for rule '{}' (\"{glob}\") matched no files{hint}",
                    entry.rule
                )
            } else {
                let path = entry.path.as_deref().unwrap_or_default();
                format!(
                    "exclude for rule '{}' (\"{path}\") matched no files — check for a typo in the path filter",
                    entry.rule
                )
            }
        })
        .collect()
}

/// Capability self-report: a top-level `exclude` (`RuleConfig::global_excludes`) whose path/glob filter
/// matches NONE of the scanned files — the same likely-typo signal as `unmatched_suppression_warnings`,
/// but worded as a top-level exclude (no rule id to name, since a global exclude is rule-agnostic). A
/// filter-less entry can't occur here (`GlobalExclude` has no bare "everywhere" shape without a path/glob —
/// unlike `Suppression`, there is no `rule` field to anchor a filter-less entry to), so every entry is
/// checked, unlike `unmatched_suppression_warnings`'s filter-less exemption.
pub(crate) fn unmatched_global_exclude_warnings(
    config: &EngineConfig,
    rels: &[&str],
) -> Vec<String> {
    config
        .rule_config
        .global_excludes
        .iter()
        .filter(|entry| {
            !rels
                .iter()
                .any(|rel| zzop_core::global_exclude_matches_path(entry, rel))
        })
        .map(|entry| {
            if let Some(glob) = &entry.glob {
                let hint = if looks_segment_bound(glob) {
                    format!(" — a leading '*' does not cross '/'; did you mean \"**/{glob}\"?")
                } else {
                    String::new()
                };
                format!("exclude \"{glob}\" matched no files{hint}")
            } else {
                let path = entry.path.as_deref().unwrap_or_default();
                format!("exclude \"{path}\" matched no files — check for a typo in the path filter")
            }
        })
        .collect()
}

/// A glob "looks segment-bound" when it has no `**` (so it cannot span `/`) and contains at least one
/// `*`/`?` — the shape that classically fails to match a nested path (e.g. `*.stories.tsx` never hits
/// `src/x.stories.tsx`). Used only to decide whether the "did you mean `**/...`?" hint applies.
fn looks_segment_bound(glob: &str) -> bool {
    !glob.contains("**") && (glob.contains('*') || glob.contains('?'))
}

/// `RuleConfig::disabled_rules` entries that match no known rule id — the substrate for
/// `DiagnosticsInput::unknown_disabled_rule_ids`. "Known" is the union of every native-analysis id
/// (built fresh here since the engine keeps no live `RuleRegistry` of its own), every `config.packs`
/// pack id, and every `"<pack>/<rule>"` id within those packs.
fn unknown_disabled_rule_ids(config: &EngineConfig) -> Vec<String> {
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut registry = zzop_core::RuleRegistry::new();
    crate::register_all_native(&mut registry);
    known.extend(registry.metas().iter().map(|m| m.id.clone()));
    for pack in &config.packs {
        known.insert(pack.id.clone());
        for rule in &pack.rules {
            known.insert(format!("{}/{}", pack.id, rule.id));
        }
    }
    config
        .rule_config
        .disabled_rules
        .iter()
        .filter(|id| !known.contains(id.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod unmatched_suppression_tests {
    use super::*;
    use zzop_core::{GlobalExclude, RuleConfig, Suppression};

    fn config_with(suppressions: Vec<Suppression>) -> EngineConfig {
        EngineConfig {
            rule_config: RuleConfig {
                suppressions,
                ..RuleConfig::default()
            },
            ..EngineConfig::default()
        }
    }

    fn config_with_global_excludes(global_excludes: Vec<GlobalExclude>) -> EngineConfig {
        EngineConfig {
            rule_config: RuleConfig {
                global_excludes,
                ..RuleConfig::default()
            },
            ..EngineConfig::default()
        }
    }

    #[test]
    fn segment_bound_glob_matching_nothing_warns_with_double_star_hint() {
        let config = config_with(vec![Suppression {
            rule: "browser/no-system-dialogs".to_string(),
            path: None,
            glob: Some("*.stories.tsx".to_string()),
        }]);
        let rels = ["src/a/x.stories.tsx"];
        let warnings = unmatched_suppression_warnings(&config, &rels);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("matched no files"));
        assert!(warnings[0].contains("**/*.stories.tsx"));
    }

    #[test]
    fn segment_bound_glob_matching_a_root_file_warns_nothing() {
        let config = config_with(vec![Suppression {
            rule: "browser/no-system-dialogs".to_string(),
            path: None,
            glob: Some("*.stories.tsx".to_string()),
        }]);
        let rels = ["x.stories.tsx"];
        let warnings = unmatched_suppression_warnings(&config, &rels);
        assert!(warnings.is_empty());
    }

    #[test]
    fn whole_rule_suppression_with_no_path_or_glob_is_never_flagged() {
        let config = config_with(vec![Suppression {
            rule: "r".to_string(),
            path: None,
            glob: None,
        }]);
        let rels = ["anything.ts"];
        let warnings = unmatched_suppression_warnings(&config, &rels);
        assert!(warnings.is_empty());
    }

    #[test]
    fn plain_path_substring_matching_nothing_warns_without_double_star_hint() {
        let config = config_with(vec![Suppression {
            rule: "r".to_string(),
            path: Some("legacy/".to_string()),
            glob: None,
        }]);
        let rels = ["src/fresh/new.ts"];
        let warnings = unmatched_suppression_warnings(&config, &rels);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("matched no files"));
        assert!(!warnings[0].contains("**/"));
    }

    #[test]
    fn global_exclude_segment_bound_glob_matching_nothing_warns_with_double_star_hint() {
        let config = config_with_global_excludes(vec![GlobalExclude {
            path: None,
            glob: Some("*.stories.tsx".to_string()),
        }]);
        let rels = ["src/a/x.stories.tsx"];
        let warnings = unmatched_global_exclude_warnings(&config, &rels);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("matched no files"));
        assert!(warnings[0].contains("**/*.stories.tsx"));
        // Worded as a top-level exclude, not tied to any rule id.
        assert!(!warnings[0].contains("rule"));
    }

    #[test]
    fn global_exclude_segment_bound_glob_matching_a_root_file_warns_nothing() {
        let config = config_with_global_excludes(vec![GlobalExclude {
            path: None,
            glob: Some("*.stories.tsx".to_string()),
        }]);
        let rels = ["x.stories.tsx"];
        let warnings = unmatched_global_exclude_warnings(&config, &rels);
        assert!(warnings.is_empty());
    }

    #[test]
    fn global_exclude_plain_path_substring_matching_nothing_warns_without_double_star_hint() {
        let config = config_with_global_excludes(vec![GlobalExclude {
            path: Some("legacy/".to_string()),
            glob: None,
        }]);
        let rels = ["src/fresh/new.ts"];
        let warnings = unmatched_global_exclude_warnings(&config, &rels);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("matched no files"));
        assert!(!warnings[0].contains("**/"));
    }

    #[test]
    fn empty_global_excludes_warns_nothing() {
        let config = config_with_global_excludes(Vec::new());
        let rels = ["anything.ts"];
        let warnings = unmatched_global_exclude_warnings(&config, &rels);
        assert!(warnings.is_empty());
    }
}
