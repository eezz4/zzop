//! Per-pack DSL applicability: the one census (`compute_dsl_scope`) that answers "which of this tree's
//! files would a loaded pack's rules even look at", and the two self-reports derived from it — the
//! tree-wide "not one loaded rule applies here" note and the per-pack "these packs matched nothing, and
//! here is the lever" advice. Split out of `capability` (same module family, same message-shape
//! contract: the strings are user-visible, extend in the existing voice, never rewrite a pinned head).
//!
//! The census is also `AnalyzeOutput::packs_loaded`'s `files_in_scope` source, so the number a consumer
//! reads on the wire and the number these warnings reason about can never be two different computations.

mod uncovered_extension;

use uncovered_extension::uncovered_extension_warning;

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
    /// `extension -> (analyzed files with it, of which in scope of >=1 loaded rule)`, over the
    /// extensions a native frontend claims ([`crate::dispatch::dispatch_by_extension`]) — the LANGUAGE
    /// axis of the same census, and [`uncovered_extension_warning`]'s only input. Extension-keyed and
    /// lowercased because the question it answers ("does any loaded rule target this filetype at all")
    /// is asked per filetype, not per file. The set of extensions considered is DERIVED from the
    /// dispatch table, never a hand-written language list: a hand-written one is the same
    /// covered-set-narrower-than-the-real-set defect this report exists to disclose, one level up.
    /// A `BTreeMap` so the warning's extension order is deterministic with no sort.
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
/// `file_exclude_pattern` is deliberately NOT consulted: it is a per-rule veto applied at evaluation
/// time, so folding it in here would make the census depend on which rule vetoed what. The count is
/// therefore an UPPER bound on candidacy — which keeps the only claim built on top of it
/// ([`zero_scope_packs_warning`]) safe in the honest direction: a zero really is "nothing matched".
pub(crate) fn compute_dsl_scope(
    packs: &[zzop_core::RulePackDef],
    analyzed_rels: &[&str],
) -> DslScope {
    // pattern string -> per-file match mask; `None` = pattern failed to compile (counts as matching
    // nothing). Computed lazily, once per unique pattern.
    let mut masks: std::collections::HashMap<&str, Option<Vec<bool>>> =
        std::collections::HashMap::new();
    let mut files_in_scope_by_pack = Vec::with_capacity(packs.len());
    let mut any_rule_applies = false;
    // Per-file union across every pack, accumulated from the same masks the per-pack counts use.
    let mut in_scope_mask = vec![false; analyzed_rels.len()];
    for pack in packs {
        let mut pack_mask = vec![false; analyzed_rels.len()];
        for rule in &pack.rules {
            let pattern = match &rule.matcher {
                zzop_core::Matcher::LineScan(m) => &m.file_pattern,
                zzop_core::Matcher::MethodScan(m) => &m.file_pattern,
                zzop_core::Matcher::SymbolScan(m) => &m.file_pattern,
                zzop_core::Matcher::IoScan(m) => &m.file_pattern,
                zzop_core::Matcher::CallScan(m) => &m.file_pattern,
                zzop_core::Matcher::LiteralScan(m) => &m.file_pattern,
            };
            let mask = masks.entry(pattern.as_str()).or_insert_with(|| {
                regex::Regex::new(pattern)
                    .ok()
                    .map(|re| analyzed_rels.iter().map(|rel| re.is_match(rel)).collect())
            });
            if let Some(mask) = mask {
                for (slot, matched) in pack_mask.iter_mut().zip(mask.iter()) {
                    *slot |= matched;
                }
            }
        }
        let count = pack_mask.iter().filter(|b| **b).count();
        any_rule_applies |= count > 0;
        for (slot, matched) in in_scope_mask.iter_mut().zip(pack_mask.iter()) {
            *slot |= matched;
        }
        files_in_scope_by_pack.push(count);
    }
    let in_scope_rels = analyzed_rels
        .iter()
        .zip(in_scope_mask.iter())
        .filter(|(_, matched)| **matched)
        .map(|(rel, _)| (*rel).to_string())
        .collect();
    // The language axis, folded out of the SAME per-file union above so it can never disagree with the
    // per-pack counts. Files no native frontend claims are skipped here on purpose: `unparsed_extension_
    // warning` already owns them, and "no rule targets .png" is not a coverage gap.
    let mut ext_census: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    for (rel, matched) in analyzed_rels.iter().zip(in_scope_mask.iter()) {
        if crate::dispatch::dispatch_by_extension(rel).is_none() {
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
        analyzed_files: analyzed_rels.len(),
        any_rule_applies,
        in_scope_rels,
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
        .collect()
}

/// Capability self-report (D16): packs loaded (`config.packs` non-empty) but not a SINGLE loaded rule's
/// `file_pattern` matches any file this tree actually analyzed — e.g. a Go-only tree loaded against
/// TS/Python-oriented packs. Without this, "112 rules loaded, 0 findings" is undiagnosable: it reads
/// identically to "112 rules loaded, ran, tree is genuinely clean". This distinguishes "no applicable
/// rules" from "clean" — native structural/whole-graph analyses still ran regardless (they are not
/// `file_pattern`-gated), so this is purely a DSL-coverage disclosure. `scope` must be the
/// [`compute_dsl_scope`] census over the SAME `packs` slice (the caller computes it once and shares it
/// with `AnalyzeOutput::packs_loaded`'s per-pack `files_in_scope`).
fn no_applicable_dsl_rule_warning(
    packs: &[zzop_core::RulePackDef],
    scope: &DslScope,
) -> Option<String> {
    if packs.is_empty() || scope.any_rule_applies {
        return None;
    }
    let total_rules: usize = packs.iter().map(|p| p.rules.len()).sum();
    Some(format!(
        "{total_rules} DSL rule(s) loaded across {pack_count} pack(s), but 0 have a `file_pattern` \
         matching any file in this tree — the loaded packs target other filetypes. Native structural/ \
         whole-graph analyses still ran; zero DSL findings in this tree means \"no applicable rules\", \
         not \"clean\".",
        pack_count = packs.len()
    ))
}

/// Capability self-report, the per-pack half of the census above: packs that loaded, ran, and had ZERO
/// files in scope. The evidence for this has always been in the output (`PackLoaded::files_in_scope`,
/// `packsLoaded[].filesInScope` on the wire) and the lever has always existed (`packs.disabled` drops a
/// whole pack — `registry::is_enabled` at the ingest/pipeline gate, before any of its rules is
/// evaluated), but nothing joined the two, so a reader had to already know both to act. This is that
/// join, and nothing more.
///
/// ## What it is allowed to claim
/// Exactly one fact: no analyzed file's path matched any of the pack's rules' `file_pattern`. That is
/// path candidacy computed by [`compute_dsl_scope`] BEFORE any content is read — it is NOT "you have no
/// redis", NOT "this pack is useless here", and NOT evidence that a stack is absent. A tree gains a
/// matching path the moment someone adds one file, and the pack is back in scope with no edit at all.
/// So the message states the path fact, names the lever, says what dropping a pack buys (the rule
/// evaluations it performs), and stops. The engine deliberately does NOT act on this itself: an engine
/// that inspected a tree for evidence of a stack and skipped packs on its own would be guessing at what
/// the caller can simply declare, and a wrong evidence vocabulary makes SECURITY rules silently not run.
///
/// ## Why one line, and three silences
/// ONE aggregated line naming every zero-scope pack, never one per pack: a small single-language repo
/// can have most of the bundled packs out of scope, and a per-pack line would turn an honest signal into
/// the wall of noise readers learn to skip. Suppressed entirely for:
/// * a pack the caller ALREADY disabled — whole-pack (`disabled_rules` carrying the bare pack id, which
///   is what the `packs.disabled` config key maps to) or every rule of it individually
///   (`"<pack>/<rule>"`, what `pipeline::gate_pack_rules` drops). `packs_loaded` cannot be read for
///   this: it reflects LOADING, not gating, so a disabled pack still appears there with
///   `files_in_scope: 0`. Nagging about a decision the reader already made is the fastest way to teach
///   them to ignore the channel.
/// * a pack with no rules at all — there is no evaluation to skip, so there is no advice.
/// * a tree that analyzed zero files ([`DslScope::analyzed_files`]), where EVERY pack is trivially
///   zero-scope and the count carries no information; `analyze::assemble`'s own "root produced 0
///   analyzable files" self-report owns that case.
///
/// Deterministic: pack ids sorted, so the same tree always produces the same line.
///
/// This does not replace [`no_applicable_dsl_rule_warning`], and is not redundant with it: that one
/// fires only when NOT ONE loaded rule applies anywhere, and it answers a different question ("is zero
/// findings clean, or is it out of scope?"). The common case here — a TypeScript repo where the Java and
/// Python packs are out of scope while the TS packs are not — leaves it silent by construction.
fn zero_scope_packs_warning(
    packs: &[zzop_core::RulePackDef],
    scope: &DslScope,
    rule_config: &zzop_core::RuleConfig,
) -> Option<String> {
    if scope.analyzed_files == 0 {
        return None;
    }
    let mut ids: Vec<&str> = packs
        .iter()
        .enumerate()
        .filter(|(i, pack)| {
            scope.files_in_scope_by_pack.get(*i).copied().unwrap_or(0) == 0
                && !pack.rules.is_empty()
                && zzop_core::registry::is_enabled(rule_config, &pack.id)
                && pack.rules.iter().any(|rule| {
                    zzop_core::registry::is_enabled(
                        rule_config,
                        &format!("{}/{}", pack.id, rule.id),
                    )
                })
        })
        .map(|(_, pack)| pack.id.as_str())
        .collect();
    if ids.is_empty() {
        return None;
    }
    ids.sort_unstable();
    let count = ids.len();
    let quoted = ids
        .iter()
        .map(|id| format!("\"{id}\""))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "{count} loaded pack(s) had 0 files in scope and still ran. No file in this tree matches any \
         of their rules' `file_pattern`, a path check made before any file content is read — so those \
         packs can only ever report zero here, which is scope, not a clean bill of health, and it \
         changes the moment a matching file is added. If this tree will never carry those stacks, \
         dropping them buys back their rule-evaluation time: `packs: {{ disabled: [{quoted}] }}` in \
         zzop.config.jsonc (embedders: `disabledRules`). Packs you already disabled are not listed \
         here."
    ))
}
