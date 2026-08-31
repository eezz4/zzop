//! Self-reports for the config keys that FILTER what a run sees: a suppression or top-level exclude
//! whose path/glob filter matched no scanned file (almost always a typo), what an exclude that DID match
//! removes from scoring, and what `vocabulary.skipDirs` removed from the walk before any of the above
//! could see it.

use crate::EngineConfig;

/// Directory names [`skipped_dirs_warning`] never reports, though the walk really did prune them. The
/// axis is NOT "this one is boring" — it is the same fact/convention split the vocabulary front end draws
/// everywhere else: every other name in a skip list is a CONVENTION a project chose and could have
/// chosen differently (a tree really can commit source under `build/`, which is the defect that produced
/// this disclosure), while these are fixed by a tool and hold a machine's own bookkeeping by
/// construction. `.git` is git's object store; the other two are zzop's own former report/cache
/// directories, kept in the default skip list as legacy defense (`dispatch::DEFAULT_SKIP_DIRS`). No
/// project can put analyzable source in them, so naming them is a prune the reader can never act on —
/// and `.git` alone would put a permanent, unactionable line in every git repository's reply, which is
/// how a warning teaches its reader to stop reading it.
///
/// The reserved `.zzop` namespace needs no entry: it is pruned ahead of the skip list entirely
/// (`pipeline::walking::walk_files`), so it never enters the prune list this filters.
const NOT_SOURCE_BY_CONSTRUCTION: &[&str] = &[".git", "zzop-reports", ".zzop-cache"];

/// Whether `rel`'s own directory name is one of [`NOT_SOURCE_BY_CONSTRUCTION`]. Keyed on the NAME, not
/// the path, so a nested `vendor/.git` (a committed submodule store) is filtered the same as a root one.
fn is_not_source_by_construction(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    NOT_SOURCE_BY_CONSTRUCTION.contains(&name)
}

/// Scope self-report: which directories `vocabulary.skipDirs` pruned from the walk. `skipped` is the
/// walk's own prune list (`pipeline::walking::Walked::skipped_dirs`), sorted and deduplicated; `None`
/// when nothing REPORTABLE was pruned (see [`NOT_SOURCE_BY_CONSTRUCTION`]), so the ordinary run stays quiet.
///
/// This is the one filter in the config whose effect was, until 2026-08-16, invisible in every channel.
/// The other three each already self-report: `exclude` names its dead filters and its scoring effect
/// above, a size-capped file lands in `coverage.degraded`, an unparsed extension gets a per-extension
/// warning. A `skipDirs` prune stops the walk at the directory, so the files under it are never counted,
/// never parsed, and never judged — the reply is not a smaller answer about the tree, it is a complete
/// answer about a different tree, and nothing said so. Measured on an external tree whose sources sat
/// under `build/`: file count 8 -> 3, findings 15 -> 2, no trace.
///
/// Leads with the distinct directory NAMES, because a name is the literal string in the config list and
/// therefore the whole edit. Example PATHS are appended only when at least one prune happened below the
/// root — for a tree whose pruned directories all sit at the top level the paths ARE the names, and
/// printing both produced `(.claude, .git): .claude, .git` in the first field run. Paths are capped at
/// [`super::SAMPLE`]; names are not, being bounded by a list an author wrote by hand.
///
/// Deliberately silent about how many FILES were lost — counting them would mean walking the directories
/// the key exists to not walk. The line names what was pruned and lets the reader decide.
///
/// `node_modules`, `dist`, `build` and the rest of the shipped list are NOT filtered out for reading as
/// routine: which name is the surprising one is exactly what this engine cannot know, and the measured
/// defect was a tree whose sources sat under `build/`. [`NOT_SOURCE_BY_CONSTRUCTION`] is the one
/// exception and it is drawn on a different axis than "boring" — see its doc. Volume stays low without
/// any further filtering, because a directory a committed `.gitignore` already excludes never reaches
/// the prune at all (sealed by `a_gitignored_directory_is_not_attributed_to_the_skip_list`), so what
/// survives to be named is the COMMITTED directory that was skipped anyway — the interesting case by
/// construction. Measured on this repo: 11 declared names, 1 reported.
pub(crate) fn skipped_dirs_warning(skipped: &[String]) -> Option<String> {
    let skipped: Vec<&String> = skipped
        .iter()
        .filter(|rel| !is_not_source_by_construction(rel))
        .collect();
    if skipped.is_empty() {
        return None;
    }
    let mut names: Vec<&str> = skipped
        .iter()
        .map(|rel| rel.rsplit('/').next().unwrap_or(rel.as_str()))
        .collect();
    names.sort_unstable();
    names.dedup();
    let nested = skipped.iter().any(|rel| rel.contains('/'));
    let where_ = if nested {
        let mut paths = skipped
            .iter()
            .take(super::SAMPLE)
            .map(|rel| rel.as_str())
            .collect::<Vec<&str>>()
            .join(", ");
        if skipped.len() > super::SAMPLE {
            paths.push_str(&format!(", +{} more", skipped.len() - super::SAMPLE));
        }
        format!(" (e.g. {paths})")
    } else {
        String::new()
    };
    // Agreement is computed, not assumed plural: the single-prune case is the common one (this repo, and
    // every tree that commits exactly one skipped directory), and "1 directory ... nothing under them"
    // reads as a message written for some other run.
    let (noun, obj, subj, contribute, its, cond) = if skipped.len() == 1 {
        (
            "directory",
            "it",
            "it",
            "contributes",
            "its",
            "that name holds",
        )
    } else {
        (
            "directories",
            "them",
            "they",
            "contribute",
            "their",
            "any of those names hold",
        )
    };
    Some(format!(
        "`vocabulary.skipDirs` pruned {} {noun} from the walk, named {}{where_}. Nothing under {obj} was \
         read, so {subj} {contribute} no file to the census, no finding to any rule, and no node or edge \
         to the import graph — {its} zero is an absence of EVIDENCE, not a clean bill, and every count in \
         this reply describes the remaining tree only. If {cond} source in this tree, declare \
         `vocabulary.skipDirs` without that name; the key replaces the list outright, so the declaration \
         must name every directory that should still be skipped.",
        skipped.len(),
        names.join(", ")
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
/// metric's subject set to speak for all of them; the message says "matched N scanned files" and lets
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
