//! Capability self-reports with a fixed message each: git-not-requested, zero-DSL-packs, and the
//! per-extension "bring an adapter" disclosure. Message strings are a user-visible contract (docs
//! and tests pin their shape) — extend in the existing voice, never rewrite the pinned head.

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
    let native_count = registry.metas().len();
    Some(format!(
        "no DSL rule packs loaded: only the {native_count} built-in native analyses ran. If you expected the bundled packs, reinstall/check the package (the bundled packs directory may be missing); to add your own, set `packs: {{ extraDirs: [...] }}` in zzop.config.jsonc (embedders: `packsDir`)."
    ))
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
        files_in_scope_by_pack.push(count);
    }
    DslScope {
        files_in_scope_by_pack,
        any_rule_applies,
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
pub(in crate::analyze) fn unparsed_extension_warning(
    unparsed: &BTreeMap<String, (usize, Vec<String>)>,
) -> Vec<String> {
    unparsed
        .iter()
        .map(|(ext, (count, sample_rels))| {
            let mut sample_str = sample_rels.join(", ");
            if *count > sample_rels.len() {
                sample_str.push_str(&format!(", +{} more", count - sample_rels.len()));
            }
            format!(
                "{count} file(s) with extension .{ext} have no native parser — no io/symbol facts were \
                 extracted from them: {sample_str}. If this language matters for the analysis, provide a \
                 Mode B adapter overlay via `overlays: [...]` in zzop.config.jsonc (embedders: \
                 `adapterOverlays`) — a partial overlay covering just the missing channel/files is \
                 enough to start (a tens-of-lines script; see the examples/ adapters in the repo \
                 (embedded: `zzop-mcp contract adapter-guide`), or `zzop-mcp contract example-envelope` \
                 for a complete sample). The contract ships inside the binary: `zzop-mcp contract \
                 envelope-guide` / MCP resource `zzop://contract/envelope-guide` (machine-checkable \
                 schema: `zzop-mcp contract envelope-schema`); repo users, see docs/NORMALIZED_AST.md. \
                 (Mode A full-envelope analysis: `zzop-mcp analyze-envelope <file>` / MCP tool \
                 `analyze_envelope`.)"
            )
        })
        .collect()
}
