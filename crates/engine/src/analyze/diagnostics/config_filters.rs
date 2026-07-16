//! Dead-filter self-reports: a suppression or top-level exclude whose path/glob filter matched no
//! scanned file (almost always a typo).

use crate::EngineConfig;

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
