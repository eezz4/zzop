//! The FILE axis of the DSL applicability census: files that are in scope by pattern and judged by
//! nothing, because every rule targeting them declines them with its own `file_exclude_pattern`. A
//! child module of `pack_scope` so it reads the parent's [`DslScope`] directly and no second copy of
//! the census can exist.

use super::DslScope;

/// Capability self-report: files a loaded rule's `file_pattern` matches — so the census counts them in
/// `files_in_scope` and `packsLoaded[].filesInScope` publishes that count — on which NOT ONE DSL rule
/// actually ran, because every rule that targets them also vetoes them through its own
/// `file_exclude_pattern`. The file was in the flow's denominator and out of its numerator, and nothing
/// said so.
///
/// ## The gap, and why no existing report held it
/// The two halves of the census are each other's blind spot, exactly at this file. `files_in_scope` is
/// pattern-only by design (an UPPER bound — [`compute_dsl_scope`](super::compute_dsl_scope) documents
/// why folding a per-rule veto into a pack-level number would be incoherent), so it counts the file.
/// [`DslScope::zero_admission_rules_by_pack`] is keyed on RULES, and the vetoing rules admitted plenty
/// of OTHER files, so the file appears in no zero-admission list. `uncovered_extension_warning` reads
/// only `in_scope == 0` extensions, and this file's extension is in scope. `scoring_scope_warning` owns
/// the files the CALLER excluded, which these are not. So `filesInScope` overstated coverage by exactly
/// this set and every channel agreed with it.
///
/// ## Its population against the BUNDLED packs is zero today, and that is measured, not assumed
/// 134 of the 144 bundled rules carry a `file_exclude_pattern` (recounted 2026-08-10): 132 the shared
/// `${test-paths}`/`${test-paths-stories}`/`${test-paths-migrations}` vocabulary, one a pack-local
/// extension of it (`reliability/sync-fs-in-handler`'s `${test-paths-stories-scripts}`), and one a
/// pattern of its own (`reliability/process-exit-in-lib`, naming `scripts?|tools|bin` only). The other
/// 10 carry NONE, and their patterns
/// blanket every extension the 133 target (`security/high-entropy-secret` alone covers
/// ts/tsx/js/mjs/cjs/py/java/rs/go/cs), so a bundled-packs-only run can never produce a fully vetoed
/// file: something always admits it. Measured — 0 on this repo's 1623 analyzed files under its own
/// committed config.
///
/// That makes this report a guard rather than a current finding, and the honest reading is that the
/// BUNDLED set is well shaped, not that the check is idle. Its live population is the pack sets this
/// engine does not author: a `packs.extraDirs` pack targeting a filetype the bundled patterns never
/// reach (`.sql` outside `migrations/`, `.prisma`, `.vue`) while carrying its own exclude. Verified end
/// to end on exactly that config — a house `.sql` pack excluding `${test-paths}` over a tree with four
/// `.sql` files on test paths reports 4, and drops to 1 once the caller declares `tests/` in `exclude`.
/// If a future bundled rule narrows one of those 11 patterns, this report starts firing on the shipped
/// set, which is the point of it existing before that day rather than after.
///
/// ## Never wider than `in_scope_rels`
/// The subject set is a strict subset of [`DslScope::in_scope_rels`] by construction, which is the same
/// discipline `minified_files_warning` documents and for the same reason: a `.md` note or a `.png`
/// asset no rule targets lost no coverage, and counting it would claim a loss that never happened. The
/// filter is not applied here at all — it is structural, because `rule_vetoed_rels` is built by
/// subtracting one mask from another mask that is itself the scope union.
///
/// ## What it may claim
/// One fact: no loaded rule's PATH gates admitted these files. It is NOT "these files are unanalyzed" —
/// the native structural/whole-graph analyses are not `file_pattern`-gated and read them in full. And it
/// closes the FILE axis only: a file a rule DID admit, whose `require_file`-family content gate then
/// rejected it, was judged (the module doc of [`rule_admission`](super::rule_admission) owns that
/// distinction) and is deliberately not counted here. The message says both out loud, because the
/// stronger reading is the same overclaim this whole channel exists to refuse.
///
/// ## Silences
/// * no packs loaded, or a tree that analyzed zero files — nothing can be in scope, so nothing can be
///   vetoed out of it; `zero_packs_warning` and the "root produced 0 analyzable files" self-report own
///   those cases.
/// * files the caller already declared in the top-level `exclude` — the caller accounted for them, and
///   `scoring_scope_warning` already states what excluding does. Re-reporting a decision the reader made
///   is how a channel teaches its reader to skip it. This subtraction is what makes the count answer
///   "what fell out that I did NOT ask to drop".
///
/// ONE aggregate entry with a [`SAMPLE`](crate::analyze::diagnostics::SAMPLE)-capped example list, never
/// one line per file: `zero_scope_packs_warning`'s doc owns that rule, and a per-file line would break
/// it harder than a per-pack line ever could.
///
/// Deterministic: [`DslScope::rule_vetoed_rels`] is sorted at the census, and pure path logic makes it
/// byte-identical warm and cold.
pub(super) fn rule_vetoed_files_warning(
    packs: &[zzop_core::RulePackDef],
    scope: &DslScope,
    rule_config: &zzop_core::RuleConfig,
) -> Option<String> {
    if packs.is_empty() || scope.analyzed_files == 0 {
        return None;
    }
    let vetoed: Vec<&str> = scope
        .rule_vetoed_rels
        .iter()
        .filter(|rel| {
            !rule_config
                .global_excludes
                .iter()
                .any(|entry| zzop_core::global_exclude_matches_path(entry, rel))
        })
        .map(String::as_str)
        .collect();
    if vetoed.is_empty() {
        return None;
    }
    // Bound, not re-declared: a second `const` here would be a second censused policy NAME
    // (`scripts/policy-census.txt` keys on `path:CONST`) for the one cap `SAMPLE` already owns.
    let cap = crate::analyze::diagnostics::SAMPLE;
    let mut sample_str = vetoed
        .iter()
        .take(cap)
        .copied()
        .collect::<Vec<&str>>()
        .join(", ");
    if vetoed.len() > cap {
        sample_str.push_str(&format!(", +{} more", vetoed.len() - cap));
    }
    Some(format!(
        "{count} file(s) match a loaded DSL rule's `file_pattern` but EVERY rule that targets them also \
         declines them through its own `file_exclude_pattern`, so not one DSL rule ran on any of them — \
         while `packsLoaded[].filesInScope` still counts them, because that field is path candidacy and \
         an upper bound. First {cap} by path: {sample_str}. The veto comes from the RULE PACK, not from \
         your own filters: files your top-level `exclude` covers are reported separately and are not \
         counted here. Zero DSL findings over these files is scope, not a clean bill of health. If \
         dropping them is what you want, say so in `exclude` (zzop.config.jsonc) — that also takes them \
         out of the scoring denominator, which a rule's veto does not. If it is not what you want, they \
         need review, and the only lever is the rule set itself: edit the pack if it is yours, or load \
         one whose rules target these files without that exclusion (`packs: {{ extraDirs: [...] }}`; \
         embedders: `packsDir`). No configuration key overrides a rule's own path veto. \
         The native structural/whole-graph analyses are not \
         `file_pattern`-gated and did read these files. This counts PATH admission only: a file a rule \
         admitted and whose `require_file`-family CONTENT gate then rejected was judged, and is not here.",
        count = vetoed.len()
    ))
}
