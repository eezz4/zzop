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

/// Everything the top-level `exclude` key self-reports, in emission order: the dead-filter warnings
/// ([`unmatched_global_exclude_warnings`]) then the scoping disclosure ([`scoring_scope_warning`]).
///
/// One entry point because both read the same config key against the same `rels`, and because the caller
/// in `analyze::assemble` lists one diagnostic per line — adding a second `exclude` self-report there took
/// that function past the 300-line ceiling `check-max-file-lines` enforces. Grouping by the config key
/// they explain keeps the next one from paying that toll again.
pub(crate) fn global_exclude_diagnostics(config: &EngineConfig, rels: &[&str]) -> Vec<String> {
    let mut out = unmatched_global_exclude_warnings(config, rels);
    out.extend(scoring_scope_warning(config, rels));
    out
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

/// Scope self-report: `exclude` stopped being presentation-only on 2026-07-30. An excluded file is no
/// longer a judged SUBJECT, so it leaves the denominator behind every per-file score, and the pain index
/// moves with it (`zzop_metrics::scores::compute::ScoresInput::is_scored`). That is what makes the number
/// actionable for anyone excluding code they cannot change — and it also means the headline figure is a
/// statement about the JUDGED population, not about the tree, which nothing in the output otherwise says.
///
/// Without this line the failure mode is a silent misreading rather than a wrong number: a reader who
/// compares two runs sees pain move and attributes it to the code, when the config moved instead. The
/// direction is not predictable either — measured on this repo, excluding `parser/`+`rules/`+`packages/`
/// took pain from 62.5 UP to 64.3, because the excluded directories were cleaner than average and the
/// remaining population is what is being described.
///
/// Emitted only when an exclude actually removed a scanned file: a config whose every exclude matched
/// nothing already gets [`unmatched_global_exclude_warnings`] above, and a count of zero would claim a
/// scoping effect that did not happen.
///
/// The count is over SCANNED FILES the filter matched, which is deliberately not the same set as
/// "judged subjects removed" — several scores additionally require source-ness and `loc > 0`, so a match
/// on an unparseable file removes nothing from them. Naming the smaller number would mean picking one
/// metric's subject set to speak for all seventeen; the message says "matched N scanned files" and lets
/// the sentence after it carry which figures actually move.
pub(crate) fn scoring_scope_warning(config: &EngineConfig, rels: &[&str]) -> Option<String> {
    if config.rule_config.global_excludes.is_empty() {
        return None;
    }
    let excluded = rels
        .iter()
        .filter(|rel| {
            config
                .rule_config
                .global_excludes
                .iter()
                .any(|entry| zzop_core::global_exclude_matches_path(entry, rel))
        })
        .count();
    if excluded == 0 {
        return None;
    }
    let plural = if excluded == 1 { "" } else { "s" };
    Some(format!(
        "`exclude` matched {excluded} scanned file{plural}, which removes them from SCORING and not just \
         from the lists — the per-file scores and the pain index they feed describe the files this config \
         judges, so they are only comparable against runs using the same `exclude`. NOT every figure \
         moves: the four slice- and module-keyed metrics have no per-file subject to exclude and keep \
         reading the whole tree, so the part of the pain index they contribute (including the cycle \
         count, its single heaviest input) is unchanged even for a cycle living entirely inside excluded \
         paths. Excluded files also stay real import targets, so no other file's fan-out moved. Note the \
         direction is not predictable: excluding code that is cleaner than average raises pain."
    ))
}

/// A glob "looks segment-bound" when it has no `**` (so it cannot span `/`) and contains at least one
/// `*`/`?` — the shape that classically fails to match a nested path (e.g. `*.stories.tsx` never hits
/// `src/x.stories.tsx`). Used only to decide whether the "did you mean `**/...`?" hint applies.
fn looks_segment_bound(glob: &str) -> bool {
    !glob.contains("**") && (glob.contains('*') || glob.contains('?'))
}
