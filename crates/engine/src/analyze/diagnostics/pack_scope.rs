//! Per-pack DSL applicability: the one census (`compute_dsl_scope`) that answers "which of this tree's
//! files would a loaded pack's rules even look at", and the two self-reports derived from it — the
//! tree-wide "not one loaded rule applies here" note and the per-pack "these packs matched nothing, and
//! here is the lever" advice. Split out of `capability` (same module family, same message-shape
//! contract: the strings are user-visible, extend in the existing voice, never rewrite a pinned head).
//!
//! The census is also `AnalyzeOutput::packs_loaded`'s `files_in_scope` source, so the number a consumer
//! reads on the wire and the number these warnings reason about can never be two different computations.

mod rule_admission;
mod scope_warnings;
mod uncovered_extension;
mod vetoed_files;
#[cfg(test)]
mod vetoed_files_tests;

use scope_warnings::{no_applicable_dsl_rule_warning, zero_scope_packs_warning};
use uncovered_extension::uncovered_extension_warning;
use vetoed_files::rule_vetoed_files_warning;

use crate::EngineConfig;

/// Per-pack DSL applicability census (D16 follow-up): for each loaded pack, how many of this tree's
/// analyzed files fall in scope of at least one of the pack's rules' `file_pattern`s
/// (`files_in_scope_by_pack`, parallel to the `packs` slice order), plus the tree-wide "any rule
/// applies at all" bit `no_applicable_dsl_rule_warning` gates on. Computed ONCE per analysis and
/// shared by that warning and `AnalyzeOutput::packs_loaded`'s per-pack `files_in_scope` count — so a
/// consumer can tell "pack loaded but 0 files in scope" (e.g. `typescript: 12 rules` on a pure-Go
/// tree) apart from "pack ran over N files and found nothing" per pack, not just tree-wide.
///
/// Cost model: each UNIQUE pattern string is compiled exactly once and scanned over the file list
/// exactly once (`masks` memoizes per pattern) — packs share a small pattern vocabulary
/// (`\.(ts|tsx)$`-style scopes recur across rules), so the old per-rule recompile (~112 compiles
/// measured on the bundled packs) collapses to the unique-pattern count. Per-pack counts are exact
/// per-file matches, not extension-representative samples: a directory-scoped pattern like
/// `(^|/)api/.*\.ts$` counts only the files actually under `api/`, which sampling one representative
/// path per extension would miscount in either direction.
pub(crate) struct DslScope {
    /// Files matching >=1 rule `file_pattern` of the pack, one entry per `packs` element, same order.
    pub(crate) files_in_scope_by_pack: Vec<usize>,
    /// Per pack (same order): the SORTED ids of rules whose own path gates admit zero analyzed files —
    /// the RULE-granularity half of this census, and `PackLoaded::zero_admission_rules`' one source.
    /// What "admitted" means (path gates only, exclude consulted, compile failures mirror evaluation)
    /// is owned by [`rule_admission`]'s module doc. Deliberately EMPTY for a pack whose own
    /// `files_in_scope` is 0: admission is a subset of pattern candidacy, so every rule of such a pack
    /// is trivially zero and the ids would repeat the pack-level fact in up to a whole pack's worth of
    /// wire (a single-language tree has most bundled packs out of scope). That also silences the
    /// zero-analyzed-files tree, where every count is trivially zero and the "root produced 0
    /// analyzable files" self-report owns the story.
    pub(crate) zero_admission_rules_by_pack: Vec<Vec<String>>,
    /// How many files the census ran over (`analyzed_rels.len()`). Kept because a per-pack zero means
    /// two different things depending on it: over a real file list it is "these patterns matched
    /// nothing here"; over an EMPTY file list every pack is trivially zero and the count carries no
    /// information at all. [`zero_scope_packs_warning`] gates on this so it never advises dropping
    /// packs on the strength of a tree that produced no files.
    analyzed_files: usize,
    /// True when any loaded rule's `file_pattern` matches any analyzed file.
    any_rule_applies: bool,
    /// Every analyzed rel matching >=1 rule `file_pattern` of ANY loaded pack — the per-file union of
    /// the per-pack masks above, i.e. "a DSL rule would have run on this file". Kept as a rel set rather
    /// than a positional mask because the caller's rel list comes out of a `HashMap` (unordered), and it
    /// is the SAME census the per-pack counts are derived from, so no second scope predicate can drift
    /// away from this one. Its consumer is `minified_files_warning`: a minified file that no rule would
    /// have matched anyway lost no DSL coverage, so reporting it as "skipped" would be a false claim.
    pub(crate) in_scope_rels: std::collections::BTreeSet<String>,
    /// The FILE-granularity hole in [`Self::in_scope_rels`], SORTED: every rel in that union which not
    /// one loaded rule's path gates ADMITTED — i.e. every rule whose `file_pattern` matches it also
    /// vetoes it with its own `file_exclude_pattern` (or has an uncompilable exclude, which makes
    /// evaluation skip the rule entirely). A strict subset of `in_scope_rels`, so an out-of-scope
    /// `.png` can never enter it. [`vetoed_files::rule_vetoed_files_warning`] is its only consumer.
    ///
    /// It exists because the two counts above are each other's blind spot. `files_in_scope` is
    /// deliberately pattern-only (an UPPER bound — see [`compute_dsl_scope`]), and
    /// `zero_admission_rules_by_pack` is keyed on RULES, so a file every targeting rule vetoes appears
    /// in no rule's zero-admission list (those rules admitted other files) while still inflating the
    /// `filesInScope` a consumer reads. Nothing joined the two, and the file read as covered.
    ///
    /// Computed from PATH GATES ONLY — the `rule_runs` mode filter is deliberately not applied. This
    /// set states a fact about patterns ("every rule that targets this path declines this path") that
    /// is true in native and envelope mode alike; the mode axis stays owned by
    /// `zero_admission_rules_by_pack`, whose own doc records the envelope-mode inversion.
    pub(crate) rule_vetoed_rels: Vec<String>,
    /// `extension -> (analyzed files with it, of which in scope of >=1 loaded rule)`, over the files a
    /// native frontend claimed IN THIS RUN ([`crate::dispatch::dispatch`], overrides included) — the
    /// LANGUAGE axis of the same census, and [`uncovered_extension_warning`]'s only input.
    /// Extension-keyed and lowercased because the question it answers ("does any loaded rule target
    /// this filetype at all") is asked per filetype, not per file. The set of extensions considered is
    /// DERIVED from dispatch, never a hand-written language list: a hand-written one is the same
    /// covered-set-narrower-than-the-real-set defect this report exists to disclose, one level up.
    /// A `BTreeMap` so the warning's extension order is deterministic with no sort.
    ///
    /// The membership test is the WHOLE dispatch (`glob_overrides` first, then the extension map), not
    /// the extension map alone. Keyed on the map alone it asked "is this extension in the table", which
    /// is a different question from "did a parser read these files" the moment a `parsers.globOverrides`
    /// entry routes a path the table does not know: such a file fell between the two reports — the
    /// "no native parser" disclosure skips it (a parser DID claim it) and this census skipped it too —
    /// so a tree that is 90% declared-route files, natively parsed and targeted by no DSL rule at all,
    /// was named by neither (pinned in `tests/integration/analyze_glob_override_disclosure.rs`).
    ext_census: std::collections::BTreeMap<String, (usize, usize)>,
}

/// Builds the [`DslScope`] census. `packs` is `config.packs` (the LOADED set, before `disabled_rules`
/// gating — same convention `AnalyzeOutput::packs_loaded`'s own doc documents: reflects load, not
/// enablement, since applicability is about scope, not disablement) and `analyzed_rels` is every file
/// this tree's walk actually visited (`analyze::assemble`'s `loc_by_path` keys / envelope's own file
/// list). Inspects every matcher kind's OWN `file_pattern` (`LineScan`/`MethodScan`/`SymbolScan`/
/// `IoScan`/`CallScan` all carry one) — more precise than `pack_loader::applies_to`'s pack-level pre-filter,
/// which deliberately treats a `SymbolScan`/`IoScan` rule's `file_pattern` as "always matches". A rule
/// whose `file_pattern` fails to compile counts as non-matching, mirroring `applies_to`'s treatment.
///
/// `file_exclude_pattern` is deliberately NOT consulted by the PACK-level counts: it is a per-rule
/// veto, so folding it into a pack-level number would make the census depend on which rule vetoed
/// what. The pack count is therefore an UPPER bound on candidacy — which keeps the claims built on
/// top of it ([`zero_scope_packs_warning`]) safe in the honest direction: a zero really is "nothing
/// matched". The RULE-level admission lists ([`DslScope::zero_admission_rules_by_pack`]) are the
/// opposite case — per rule there is exactly one exclude and it MUST be consulted, or an all-vetoed
/// rule would read as covered; [`rule_admission`]'s module doc owns that definition.
pub(crate) fn compute_dsl_scope(
    packs: &[zzop_core::RulePackDef],
    analyzed_rels: &[&str],
    dispatch: &crate::DispatchConfig,
) -> DslScope {
    compute_dsl_scope_filtered(packs, analyzed_rels, dispatch, |_| true)
}

/// [`compute_dsl_scope`] with a mode filter: `rule_runs` says whether THIS analysis mode evaluates a
/// rule with that matcher at all. Envelope mode passes `envelope::rule_runs_in_envelope_mode`
/// (evaluation there retains only `SymbolScan`/`IoScan` rules); native mode runs everything, which is
/// what the unfiltered wrapper above says. A rule the mode never evaluates is counted ZERO-ADMISSION
/// regardless of what its path gates match — it read nothing, so its green is vacuous, which is
/// exactly the fact `zeroAdmissionRules` exists to disclose (measured before this filter existed: in
/// envelope mode the census INVERTED — the only unlisted security rules were two line-scan rules
/// that never ran, while dozens of merely-out-of-path-scope rules were listed). The PACK-level
/// `files_in_scope` deliberately still counts a non-running rule's `file_pattern`: that field claims
/// path candidacy ("a file matching at least one rule `file_pattern`"), which is a fact of the paths
/// whatever the mode runs, and per-rule admission stays a subset of it.
pub(crate) fn compute_dsl_scope_filtered(
    packs: &[zzop_core::RulePackDef],
    analyzed_rels: &[&str],
    dispatch: &crate::DispatchConfig,
    rule_runs: impl Fn(&zzop_core::Matcher) -> bool,
) -> DslScope {
    // pattern string -> per-file match mask; `None` = pattern failed to compile (counts as matching
    // nothing). Computed lazily, once per unique pattern, and SHARED with the rule-level admission
    // counts below so the two granularities read one set of match facts.
    let mut masks: rule_admission::MaskMemo<'_> = std::collections::HashMap::new();
    let mut files_in_scope_by_pack = Vec::with_capacity(packs.len());
    let mut zero_admission_rules_by_pack = Vec::with_capacity(packs.len());
    let mut any_rule_applies = false;
    // Per-file union across every pack, accumulated from the same masks the per-pack counts use.
    let mut in_scope_mask = vec![false; analyzed_rels.len()];
    // Its exclude-aware twin: files at least one rule's path gates ADMITTED. Folded across every pack
    // by the same `fold_admitted` call that produces the per-rule admission counts, so the file-axis
    // and rule-axis halves of this census cannot disagree.
    let mut admitted_mask = vec![false; analyzed_rels.len()];
    for pack in packs {
        let mut pack_mask = vec![false; analyzed_rels.len()];
        let mut zero_admitted: Vec<String> = Vec::new();
        for rule in &pack.rules {
            let (pattern, exclude) = rule_admission::path_gates(&rule.matcher);
            rule_admission::ensure_mask(&mut masks, pattern, analyzed_rels);
            if let Some(ex) = exclude {
                rule_admission::ensure_mask(&mut masks, ex, analyzed_rels);
            }
            // The pack-level count stays pattern-only (the upper-bound contract documented above);
            // only the rule-level admission consults the rule's own exclude.
            if let Some(Some(mask)) = masks.get(pattern) {
                for (slot, matched) in pack_mask.iter_mut().zip(mask.iter()) {
                    *slot |= matched;
                }
            }
            // The path-gate fold runs unconditionally: `admitted_mask` is a statement about PATTERNS
            // (`DslScope::rule_vetoed_rels`' doc owns why it ignores the mode), while the zero-
            // admission list below is a statement about this run and applies the mode filter on top.
            let admitted =
                rule_admission::fold_admitted(&masks, pattern, exclude, &mut admitted_mask);
            // Path-gate admission AND the mode filter: a rule this mode never evaluates admits
            // nothing whatever its patterns match (see `compute_dsl_scope_filtered`'s doc).
            if !rule_runs(&rule.matcher) || admitted == 0 {
                zero_admitted.push(rule.id.clone());
            }
        }
        let count = pack_mask.iter().filter(|b| **b).count();
        any_rule_applies |= count > 0;
        for (slot, matched) in in_scope_mask.iter_mut().zip(pack_mask.iter()) {
            *slot |= matched;
        }
        files_in_scope_by_pack.push(count);
        // See `DslScope::zero_admission_rules_by_pack`: a zero-scope pack (which an empty tree makes
        // every pack) lists no ids — the pack-level zero already carries the whole fact.
        if count == 0 {
            zero_admitted.clear();
        }
        zero_admitted.sort_unstable();
        zero_admission_rules_by_pack.push(zero_admitted);
    }
    let in_scope_rels = analyzed_rels
        .iter()
        .zip(in_scope_mask.iter())
        .filter(|(_, matched)| **matched)
        .map(|(rel, _)| (*rel).to_string())
        .collect();
    // In scope by pattern, admitted by nothing. Sorted explicitly because `analyzed_rels` arrives in
    // `HashMap` key order (`loc_by_path.keys()`), which would make the warning's sample paths differ
    // between two runs over the identical tree — `in_scope_rels` gets the same ordering for free from
    // its `BTreeSet`, and this Vec must not be the one place that forgets it.
    let mut rule_vetoed_rels: Vec<String> = analyzed_rels
        .iter()
        .zip(in_scope_mask.iter())
        .zip(admitted_mask.iter())
        .filter(|((_, in_scope), admitted)| **in_scope && !**admitted)
        .map(|((rel, _), _)| (*rel).to_string())
        .collect();
    rule_vetoed_rels.sort_unstable();
    // The language axis, folded out of the SAME per-file union above so it can never disagree with the
    // per-pack counts. Files no native frontend claims are skipped here on purpose: `unparsed_extension_
    // warning` already owns them, and "no rule targets .png" is not a coverage gap. The claim test is the
    // WHOLE dispatch, so a path `parsers.globOverrides` routed counts as the parsed source it is — see
    // `DslScope::ext_census`'s doc for the hole the extension map alone left between the two reports.
    let mut ext_census: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    for (rel, matched) in analyzed_rels.iter().zip(in_scope_mask.iter()) {
        if crate::dispatch::dispatch(rel, dispatch).is_none() {
            continue;
        }
        let Some(ext) = std::path::Path::new(rel)
            .extension()
            .and_then(|e| e.to_str())
        else {
            continue;
        };
        let entry = ext_census.entry(ext.to_ascii_lowercase()).or_insert((0, 0));
        entry.0 += 1;
        if *matched {
            entry.1 += 1;
        }
    }
    DslScope {
        files_in_scope_by_pack,
        zero_admission_rules_by_pack,
        analyzed_files: analyzed_rels.len(),
        any_rule_applies,
        in_scope_rels,
        rule_vetoed_rels,
        ext_census,
    }
}

/// Every applicability self-report for one analysis, in fixed order and widest-scope first: the
/// tree-wide [`no_applicable_dsl_rule_warning`], then the per-pack [`zero_scope_packs_warning`], then
/// the per-language [`uncovered_extension_warning`]. One entry point because they read the SAME `scope`
/// census over the SAME pack set and every caller wants all of them — a caller that took them separately
/// could silently emit one and forget the others, which is exactly how `analyze_envelope` used to drift
/// from `analyze::assemble`. Any subset may be absent; the returned `Vec` is empty when none has
/// anything to say. They do not overlap: the first fires only when NOTHING applies anywhere (and
/// silences the third by construction), the second is keyed on packs, the third on filetypes.
pub(crate) fn pack_scope_warnings(config: &EngineConfig, scope: &DslScope) -> Vec<String> {
    no_applicable_dsl_rule_warning(&config.packs, scope)
        .into_iter()
        .chain(zero_scope_packs_warning(
            &config.packs,
            scope,
            &config.rule_config,
        ))
        .chain(uncovered_extension_warning(&config.packs, scope))
        .chain(rule_vetoed_files_warning(
            &config.packs,
            scope,
            &config.rule_config,
        ))
        .collect()
}

// The two self-reports built on this census (`no_applicable_dsl_rule_warning`,
// `zero_scope_packs_warning`) live in `scope_warnings` — split on the file-line cap; their
// message-shape contracts are documented there.
