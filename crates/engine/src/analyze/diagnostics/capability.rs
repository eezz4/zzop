//! Capability self-reports, one fixed message shape each: git-not-requested, zero-DSL-packs,
//! dead-rule (uncompilable OR structurally empty), no-applicable-DSL-rule, and the per-extension
//! "bring an adapter" disclosure. Message strings are a user-visible contract (docs and tests pin
//! their shape) — extend in the existing voice, never rewrite the pinned head.
//!
//! Reach differs by report: the dead-rule and no-applicable ones are config-derived, so they fire on
//! `analyze_envelope` too; git-not-requested and the per-extension disclosure need a filesystem walk
//! that path never performs. `docs/modules/facade.md` states that split for consumers.

use std::collections::BTreeMap;

use crate::EngineConfig;

/// Capability self-report: git history was never requested (`config.git` is `None`), so every
/// git-derived output channel is null. Distinct from `collect_git`'s own warning, which fires only when
/// git WAS requested but collection failed — a consumer can always tell "never asked" apart from
/// "asked, failed" by which of the two strings is present. Returns `None` when git was requested.
pub(in crate::analyze) fn git_not_requested_warning(config: &EngineConfig) -> Option<String> {
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
    let native_count = registry.ids().len();
    Some(format!(
        "no DSL rule packs loaded: only the {native_count} built-in native analyses ran. If you expected the bundled packs, reinstall/check the package (the bundled packs directory may be missing); to add your own, set `packs: {{ extraDirs: [...] }}` in zzop.config.jsonc (embedders: `packsDir`)."
    ))
}

/// Every loaded rule whose own pattern will not compile, as one warning line each. A rule like that is
/// not "quiet" — it is DEAD: the evaluator skips it and the run reports clean, which is exactly the
/// misleading-diagnosis failure this engine refuses to commit. `validate-rule-pack` catches it ahead of a
/// scan, but nothing forces that call: an inline `packDefs` entry, a `packsDir` pack, or a bundled pack
/// all reach the evaluator unvalidated.
///
/// Derived from `zzop_core::pack_regex_issues`, the same judgment `validate-rule-pack` reports (regex
/// fields AND the two structural dead-rule shapes), over the LOADED pack set — so it names a rule that
/// can never fire even when no file this run visited would have matched its `file_pattern` anyway.
/// (`zzop_core::dsl::RuleDiag` reports the narrower run-scoped fact from the compile sites themselves;
/// see `dsl::eval_pack`'s doc for why the shipped engine discloses at load scope instead.)
///
/// `packs` is the LOADED set, before `disabled_rules` gating — the same convention `compute_dsl_scope`
/// documents. So a rule inside a wholly disabled pack is still named here: the message says only that
/// THIS rule cannot fire, never that the rest of the pack is running.
///
/// One line per bad FIELD, not per rule: a rule with two uncompilable patterns produces two lines, each
/// naming its own field. That is the shape `validate-rule-pack` has always emitted and what an author
/// fixing patterns one at a time wants.
pub(crate) fn uncompilable_rule_warnings(packs: &[zzop_core::RulePackDef]) -> Vec<String> {
    packs
        .iter()
        .flat_map(zzop_core::pack_regex_issues)
        .map(|issue| {
            format!(
                "{issue} — that rule is SKIPPED and can never fire; the pack's other rules are unaffected by it. Run `zzop validate-rule-pack <pack.json>` to catch this before a scan."
            )
        })
        .collect()
}

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
    /// True when any loaded rule's `file_pattern` matches any analyzed file.
    any_rule_applies: bool,
    /// Every analyzed rel matching >=1 rule `file_pattern` of ANY loaded pack — the per-file union of
    /// the per-pack masks above, i.e. "a DSL rule would have run on this file". Kept as a rel set rather
    /// than a positional mask because the caller's rel list comes out of a `HashMap` (unordered), and it
    /// is the SAME census the per-pack counts are derived from, so no second scope predicate can drift
    /// away from this one. Its consumer is `minified_files_warning`: a minified file that no rule would
    /// have matched anyway lost no DSL coverage, so reporting it as "skipped" would be a false claim.
    pub(crate) in_scope_rels: std::collections::BTreeSet<String>,
}

/// Builds the [`DslScope`] census. `packs` is `config.packs` (the LOADED set, before `disabled_rules`
/// gating — same convention `AnalyzeOutput::packs_loaded`'s own doc documents: reflects load, not
/// enablement, since applicability is about scope, not disablement) and `analyzed_rels` is every file
/// this tree's walk actually visited (`analyze::assemble`'s `loc_by_path` keys / envelope's own file
/// list). Inspects every matcher kind's OWN `file_pattern` (`LineScan`/`MethodScan`/`SymbolScan`/
/// `IoScan` all carry one) — more precise than `pack_loader::applies_to`'s pack-level pre-filter,
/// which deliberately treats a `SymbolScan`/`IoScan` rule's `file_pattern` as "always matches". A rule
/// whose `file_pattern` fails to compile counts as non-matching, mirroring `applies_to`'s treatment.
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
    DslScope {
        files_in_scope_by_pack,
        any_rule_applies,
        in_scope_rels,
    }
}

/// Capability self-report (D16): packs loaded (`config.packs` non-empty) but not a SINGLE loaded rule's
/// `file_pattern` matches any file this tree actually analyzed — e.g. a Go-only tree loaded against
/// TS/Python-oriented packs. Without this, "112 rules loaded, 0 findings" is undiagnosable: it reads
/// identically to "112 rules loaded, ran, tree is genuinely clean". This distinguishes "no applicable
/// rules" from "clean" — native structural/whole-graph analyses still ran regardless (they are not
/// `file_pattern`-gated), so this is purely a DSL-coverage disclosure. `scope` must be the
/// [`compute_dsl_scope`] census over the SAME `packs` slice (the caller computes it once and shares it
/// with `AnalyzeOutput::packs_loaded`'s per-pack `files_in_scope`).
pub(crate) fn no_applicable_dsl_rule_warning(
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

/// Capability self-report: the "bring an adapter" per-extension disclosure — one line per distinct
/// extension among files `dispatch::dispatch` returned `None` for, that are not a non-source extension
/// (`dispatch::is_non_source_extension`) and not already covered by an adapter overlay (the overlay IS the
/// parser for those; see `analyze::assemble`'s collection site for the overlay-exclusion rationale). Before
/// this change, such a file vanished from every self-report: `degraded: false`, no `io`/symbols, extension
/// recorded nowhere — this closes that gap without naming a rule/language vocabulary, only a raw extension
/// and a count. `unparsed` must already carry each extension's TOTAL count in `.0` and its first (in
/// artifact-visitation, i.e. `rel`-sorted) up-to-3 sample paths in `.1` — the caller (`analyze::assemble`)
/// caps the sample during collection rather than here, so a huge tree never holds more than 3 rels per
/// extension in memory. A `BTreeMap` key order makes the returned `Vec` deterministic (extension-ascending)
/// with no sort needed here. No-extension files (README, Dockerfile) are deliberately excluded from
/// `unparsed` altogether by the collection site, not here — see that site's own doc for why (ambiguous by
/// construction: often config/docs, no reliable language signal).
///
/// ## One fact line per extension, ONE guidance line per run
/// The adapter on-ramp ([`adapter_on_ramp_note`]) is emitted ONCE, as the last entry, instead of being
/// repeated inside every per-extension line. A field run on a repo with `.env.development`/`.env.example`/
/// `.env.production`/`.sh` printed the entire four-sentence prescriptive tail four times over — the same
/// remedy restated until it read as noise, which is how a genuine capability gap loses the reader. The
/// per-extension entries keep only their own facts (count, extension, sample paths); the funnel is not
/// weakened, only de-duplicated — see [`adapter_on_ramp_note`] for the reachability contract it carries.
pub(in crate::analyze) fn unparsed_extension_warning(
    unparsed: &BTreeMap<String, (usize, Vec<String>)>,
) -> Vec<String> {
    if unparsed.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<String> = unparsed
        .iter()
        .map(|(ext, (count, sample_rels))| {
            let mut sample_str = sample_rels.join(", ");
            if *count > sample_rels.len() {
                sample_str.push_str(&format!(", +{} more", count - sample_rels.len()));
            }
            format!(
                "{count} file(s) with extension .{ext} have no native parser — no io/symbol facts were \
                 extracted from them: {sample_str}."
            )
        })
        .collect();
    out.push(adapter_on_ramp_note(unparsed));
    out
}

/// Extensions named inline in the single on-ramp note before it collapses to a `+N more` count — the note
/// points at the per-extension entries above it, so it never needs the full list.
const ON_RAMP_EXT_SAMPLE: usize = 5;

/// The gap-to-creation funnel, stated once per run (`output-philosophy`, §2 capability gaps): a gap must
/// not end at disclosure — it chains the reader to BUILDING an adapter, and the default on-ramp is a
/// minimal Mode B overlay, never a full parser. Every named surface must be one a reader can actually
/// reach, in BOTH dialects (a binary-only MCP user has no `examples/` or `docs/` checkout, so each repo
/// path carries an embedded-contract twin): the guide (`contract envelope-guide`), the checker
/// (`validate_envelope` / `zzop validate-envelope`), and a runnable example (`contract example-envelope`).
/// Dropping one is a partial-claim regression; naming an unreachable one is the same regression in the
/// other direction (the removed napi `analyzeEnvelope` binding is deliberately absent). Pinned by
/// `unparsed_extension_tests`.
fn adapter_on_ramp_note(unparsed: &BTreeMap<String, (usize, Vec<String>)>) -> String {
    let named: Vec<String> = unparsed
        .keys()
        .take(ON_RAMP_EXT_SAMPLE)
        .map(|ext| format!(".{ext}"))
        .collect();
    let more = unparsed.len() - named.len();
    let more_note = if more > 0 {
        format!(", +{more} more")
    } else {
        String::new()
    };
    format!(
        "No native parser exists for {} extension(s) in this tree ({}{more_note}) — one entry above per \
         extension with its own count and sample paths. If any of those languages matter for the analysis, \
         provide a Mode B adapter overlay via `overlays: [...]` in zzop.config.jsonc (embedders: \
         `adapterOverlays`) — a partial overlay covering just the missing channel/files is enough to \
         start (a tens-of-lines script; see the examples/ adapters in the repo (embedded: `zzop contract \
         adapter-guide`), or `zzop contract example-envelope` for a complete sample). The contract ships \
         inside the binary: `zzop contract envelope-guide` / MCP resource \
         `zzop://contract/envelope-guide` (machine-checkable schema: `zzop contract envelope-schema`; \
         check your overlay against it with `zzop validate-envelope <file>` / MCP tool \
         `validate_envelope` before wiring it in); repo users, see docs/NORMALIZED_AST.md. (Mode A \
         full-envelope analysis: `zzop analyze-envelope <file>` / MCP tool `analyze_envelope`.)",
        unparsed.len(),
        named.join(", ")
    )
}
