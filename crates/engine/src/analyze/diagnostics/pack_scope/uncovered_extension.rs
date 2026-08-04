//! The LANGUAGE axis of the DSL applicability census: filetypes this tree is largely made of that not
//! one loaded DSL rule targets. A child module of `pack_scope` rather than a sibling, so it reads the
//! parent's [`DslScope`] fields directly and no second copy of the census can exist.

use super::DslScope;

/// Share of the analyzed file count an extension must hold before [`uncovered_extension_warning`] will
/// name it. A language-level coverage gap on a filetype that is 1% of the tree is a curiosity; on one
/// that is most of the tree, a zero DSL finding count is a misleading headline. 10 is the line between
/// the two, chosen so the report speaks about a tree's PRINCIPAL languages and stays quiet about the
/// stray fixture file: measured on this repo's own committed config (1397 analyzed files) it names
/// `.rs` (985 files, 70%) and nothing else -- `.ts` (10.9%) is covered by the shipped packs, and
/// `.py`/`.java`/`.tsx` sit under 1% each. Raising it hides real gaps in polyglot trees; lowering it
/// re-admits the fixture noise it exists to keep out. Named and censused
/// (`scripts/policy-census.txt`) rather than inlined so that moving it is a visible decision.
const MIN_UNCOVERED_EXTENSION_SHARE_PCT: usize = 10;

/// Capability self-report: a filetype holding at least [`MIN_UNCOVERED_EXTENSION_SHARE_PCT`]% of this
/// tree's analyzed files, which zzop has a native parser for, and which NOT ONE loaded DSL rule's
/// `file_pattern` targets. This repo is the case that motivated it -- 985 of its 1397 files are `.rs`,
/// the bundled packs carry no `.rs` `file_pattern` at all, and every existing report stayed silent:
/// `no_applicable_dsl_rule_warning` because the tree's `.ts` files DO match, `zero_scope_packs_warning`
/// because most packs therefore have non-zero scope, and `unparsed_extension_warning` because `.rs`
/// parses perfectly well. "0 findings" read as "clean".
///
/// ## What it may claim, and the overclaim it must not make
/// Exactly one fact: no loaded DSL rule's `file_pattern` matches any file carrying that extension. It is
/// NOT "this language is not analyzed" -- the native structural/whole-graph analyses (dep graph, cycles,
/// dead code, the cache-lane audit) are not `file_pattern`-gated and ran over those files in full, and
/// the files were walked, parsed and scored like any other. Stating the stronger thing would be this
/// very defect class pointed the other way: a claim wider than what was measured. So the message names
/// the DSL packs specifically, says out loud that the native analyses did cover the files, and stops.
///
/// ## Silences
/// * no packs loaded -- `zero_packs_warning` owns that, and "no rule targets .rs" is not news when
///   there are no rules at all.
/// * not one loaded rule applies anywhere ([`DslScope::any_rule_applies`]) -- the tree-wide
///   `no_applicable_dsl_rule_warning` already says it, and restating it per extension is the wall of
///   noise readers learn to skip.
/// * a tree that analyzed zero files, where every share is undefined.
/// * extensions no native frontend claims -- dropped by the census in `compute_dsl_scope` itself, since
///   `unparsed_extension_warning` reports those with their own counts and a `.png` has no DSL coverage
///   to lose.
///
/// Like `zero_scope_packs_warning` this reflects LOAD, not enablement: a pack the caller disabled on
/// purpose still counts as targeting its filetypes, because disabling is a decision the reader already
/// made and this report is about what the loaded RULE SET covers, not about what this run chose to run.
/// ONE aggregated line naming every qualifying extension, never one line per extension -- same reason.
pub(super) fn uncovered_extension_warning(
    packs: &[zzop_core::RulePackDef],
    scope: &DslScope,
) -> Option<String> {
    if packs.is_empty() || scope.analyzed_files == 0 || !scope.any_rule_applies {
        return None;
    }
    let uncovered: Vec<(&str, usize, usize)> = scope
        .ext_census
        .iter()
        .filter(|(_, (_, in_scope))| *in_scope == 0)
        .map(|(ext, (total, _))| (ext.as_str(), *total, total * 100 / scope.analyzed_files))
        .filter(|(_, _, share)| *share >= MIN_UNCOVERED_EXTENSION_SHARE_PCT)
        .collect();
    if uncovered.is_empty() {
        return None;
    }
    let listed = uncovered
        .iter()
        .map(|(ext, total, share)| format!(".{ext} ({total} file(s), {share}% of this tree)"))
        .collect::<Vec<_>>()
        .join(", ");
    let total_rules: usize = packs.iter().map(|p| p.rules.len()).sum();
    Some(format!(
        "NO loaded DSL rule targets {count} of this tree's principal filetype(s): {listed}. \
         zzop has a native parser for them and read them, but none of the {total_rules} rule(s) across \
         {pack_count} loaded pack(s) carries a `file_pattern` matching even one such file -- so the DSL \
         half of this run could not have reported anything about them, and zero DSL findings over them \
         is scope, not a clean bill of health. This is about the DSL rule packs ONLY: the native \
         structural/whole-graph analyses are not `file_pattern`-gated and did cover these files. It is \
         a path check made before any file content is read, and it changes the moment a pack whose \
         rules target them is loaded (`packs: {{ extraDirs: [...] }}` in zzop.config.jsonc; embedders: \
         `packsDir`). Filetypes under {MIN_UNCOVERED_EXTENSION_SHARE_PCT}% of this tree, and filetypes \
         with no native parser at all (reported separately with their own counts), are not listed here.",
        count = uncovered.len(),
        pack_count = packs.len()
    ))
}
