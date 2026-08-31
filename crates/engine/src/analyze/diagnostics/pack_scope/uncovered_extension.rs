//! The LANGUAGE axis of the DSL applicability census, in two degrees. [`uncovered_extension_warning`]
//! reports the filetypes this tree is largely made of that not ONE loaded rule targets;
//! [`thin_rule_reach_warning`] reports the ones a handful of rules reach — the same misreading a step
//! milder, and the far commoner one. A child module of `pack_scope` rather than a sibling, so both read
//! the parent's [`DslScope`] fields directly and no second copy of the census can exist.

use super::DslScope;

/// Share of the analyzed file count an extension must hold before [`uncovered_extension_warning`] will
/// name it. A language-level coverage gap on a filetype that is 1% of the tree is a curiosity; on one
/// that is most of the tree, a zero DSL finding count is a misleading headline. 10 is the line between
/// the two, chosen so the report speaks about a tree's PRINCIPAL languages and stays quiet about the
/// stray fixture file. It gates BOTH reports in this module — "is this a language the tree is made of"
/// is one question and must not get two answers. Measured on this repo's own committed config, `.rs` is
/// the only extension past it (1335 of 1879 files, 71%, re-read 2026-08-31) while `.py`/`.java`/`.tsx`
/// sit under 1% each; `.rs` is now carried by [`thin_rule_reach_warning`] rather than the zero-reach
/// report below, since the 2026-07-28 Rust batch took its reach from 0 to 20 of the shipped rules —
/// which is what the handoff between the two degrees looks like in practice. Every figure in this
/// sentence is a property of THIS TREE and moves with it: recount with `zzop analyze .` from the repo
/// root and read `fileCount` plus the `rule(s) in range` warning back. The DENOMINATOR is deliberately
/// not restated — it is the shipped rule count, which [`THIN_RULE_REACH_PCT`]'s table owns, and the
/// stale `116` that stood here was the last surviving half of the mixed-tree row that table retracted.
/// Raising it hides real gaps in polyglot trees;
/// lowering it re-admits the fixture noise it exists to keep out. Named and censused
/// (`scripts/policy-census.txt`) rather than inlined so that moving it is a visible decision.
///
/// `pub` and re-exported from the crate root (2026-08-20) because a THIRD report now asks the same
/// question: `queryCoverage`'s `unreadExtensions` cell, naming the principal filetypes NO structural
/// parser read. "Is this a language the tree is made of" must keep ONE answer across the product, and
/// a second declaration in the facade would be a second censused name for one policy — exactly what
/// `diagnostics.rs`'s module doc refuses for `SAMPLE`.
pub const MIN_UNCOVERED_EXTENSION_SHARE_PCT: usize = 10;

/// Capability self-report: a filetype holding at least [`MIN_UNCOVERED_EXTENSION_SHARE_PCT`]% of this
/// tree's analyzed files, which zzop has a native parser for, and which NOT ONE loaded DSL rule's
/// `file_pattern` targets. This repo is the case that motivated it -- it is the overwhelmingly `.rs`
/// tree [`MIN_UNCOVERED_EXTENSION_SHARE_PCT`] measures (that constant owns the share and the recount;
/// a third hand copy of it stood here, two readings stale, until 2026-08-31), at a time when
/// the bundled packs carried no `.rs` `file_pattern` at all, and every existing report stayed silent:
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

/// Share of the LOADED rule set that must be able to reach a principal filetype before this run stops
/// calling that filetype thinly covered. Re-measured 2026-08-17 against the 118 bundled rules
/// (reach = rules whose path gates admit a file of that extension):
/// `.ts` 95, `.java` 23, `.rs` 19, `.py` 13, `.go` 13, `.cs` 10 — the numbers THIS CODE reports, and
/// the recount command is to run the analysis over a tree holding only that extension and read the
/// `rule(s) in range` warning back (`.ts` returns none, being the one language above the threshold).
/// Re-verified 2026-08-31, all six unchanged, by two independent routes: that recount command for the
/// five below the threshold, and — for `.ts`, which the command structurally cannot return — by
/// expanding every shipped rule's `file_pattern` through the fragment maps and testing it against
/// `a.<ext>`, which agrees with all six. THIS ROW HAS ONE OWNER: it is deliberately absent from
/// `scripts/policy-census.txt`, whose copy of the retracted reading survived there unchecked because
/// that guard compares key and axis and never reads a tail.
/// The previous row read `.java` 25, `.py` 10, `.go` 10, `.cs` 9 and did not reproduce; the cause is
/// worth keeping because it is a measurement hazard rather than a typo. REACH IS A PROPERTY OF THE
/// TREE, NOT OF THE RULE SET: a rule whose `file_pattern` carries a path anchor admits or refuses
/// depending on the directories a tree actually has, so one unchanged rule set scores differently on a
/// synthetic single-language tree than on a real repository. The commit that published that row
/// contains both readings for one language — `.rs` 19 in the table and "20 of 116" in its prose,
/// the latter measured on THIS repository — so the row mixed trees, which is why four entries were
/// off and the two taken from the synthetic tree looked right. Numbers here must come from one tree
/// shape, and the sentence above says which. 25 is the line those numbers already
/// draw — TypeScript, the language the packs were written in, sits far above it and stays quiet, while
/// every language that gets a corner of the rule set falls under. Raising it would make the well-covered
/// case noisy; lowering it past ~8% would silence C#, the thinnest-covered language shipped, which is
/// the case this report exists for. Named and censused (`scripts/policy-census.txt`) rather than
/// inlined so that moving it is a visible decision.
const THIN_RULE_REACH_PCT: usize = 25;

/// Capability self-report: a principal filetype (same [`MIN_UNCOVERED_EXTENSION_SHARE_PCT`] share gate
/// as above — one answer to "is this a language this tree is made of") that IS targeted by some loaded
/// rule, but by at most [`THIN_RULE_REACH_PCT`]% of them.
///
/// [`uncovered_extension_warning`] above catches the total miss and stops there, so a filetype one rule
/// reaches passed the only language gate the census had. That gate is far too coarse for what was
/// measured: a Python tree is reached by the `.py` entry of [`THIN_RULE_REACH_PCT`]'s table above and
/// by no rule outside it, so the whole rest of the loaded set is structurally incapable of reporting
/// anything about it — and the run said `findings.total: 3` with all three from the one
/// TypeScript file in the tree. Nothing in the reply distinguished "your Python is clean" from "almost
/// nothing looked at your Python".
///
/// ## What it may claim
/// One fact, the same path fact its sibling reports: how many loaded rules' `file_pattern` admit a file
/// of that extension. It is NOT a claim that the language is unanalyzed — the native structural analyses
/// are not `file_pattern`-gated and covered those files in full, and the rules that DID reach them
/// really did judge them. Stating the stronger thing would be this defect class pointed the other way.
///
/// ## Silences
/// * everything [`uncovered_extension_warning`]'s own silences cover, for the same reasons.
/// * a filetype with ZERO reach — that one belongs to the sibling above, and two lines about one
///   extension is how a reader learns to read neither.
pub(super) fn thin_rule_reach_warning(
    packs: &[zzop_core::RulePackDef],
    scope: &DslScope,
) -> Option<String> {
    if packs.is_empty() || scope.analyzed_files == 0 || !scope.any_rule_applies {
        return None;
    }
    let total_rules: usize = packs.iter().map(|p| p.rules.len()).sum();
    if total_rules == 0 {
        return None;
    }
    let thin: Vec<(&str, usize, usize, usize)> = scope
        .ext_census
        .iter()
        .map(|(ext, (total, _))| {
            let reach = scope.ext_rule_reach.get(ext).copied().unwrap_or(0);
            (
                ext.as_str(),
                *total,
                *total * 100 / scope.analyzed_files,
                reach,
            )
        })
        .filter(|(_, _, share, reach)| {
            *share >= MIN_UNCOVERED_EXTENSION_SHARE_PCT
                && *reach > 0
                && reach * 100 / total_rules <= THIN_RULE_REACH_PCT
        })
        .collect();
    if thin.is_empty() {
        return None;
    }
    let listed = thin
        .iter()
        .map(|(ext, total, share, reach)| {
            format!(".{ext} ({total} file(s), {share}% of this tree): {reach} rule(s) in range")
        })
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "THIN DSL rule reach on {count} of this tree's principal filetype(s), out of {total_rules} \
         rule(s) across {pack_count} loaded pack(s) — {listed}. The rest carry a `file_pattern` no path \
         with that extension can match, so they could not have reported anything about those files \
         whatever the code contains: a low or zero DSL finding count over them is REACH, not a clean \
         bill of health. The rules that are in range did judge them normally, and the native \
         structural/whole-graph analyses are not `file_pattern`-gated and covered every file. This is a \
         path check made before any file content is read; loading packs whose rules target these \
         filetypes changes it (`packs: {{ extraDirs: [...] }}` in zzop.config.jsonc; embedders: \
         `packsDir`). Filetypes under {MIN_UNCOVERED_EXTENSION_SHARE_PCT}% of this tree are not listed, \
         and one no loaded rule reaches AT ALL is reported separately.",
        count = thin.len(),
        pack_count = packs.len()
    ))
}
