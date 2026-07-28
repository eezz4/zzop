//! Capability self-reports, one fixed message shape each: git-not-requested, zero-DSL-packs,
//! dead-rule (uncompilable OR structurally empty), and the per-extension "bring an adapter"
//! disclosure. Message strings are a user-visible contract (docs and tests pin their shape) — extend
//! in the existing voice, never rewrite the pinned head. The two pack-APPLICABILITY reports and the
//! census behind them live in the sibling `pack_scope` module.
//!
//! Reach differs by report: the dead-rule one is config-derived, so it fires on `analyze_envelope`
//! too (as do both of `pack_scope`'s); git-not-requested and the per-extension disclosure need a
//! filesystem walk that path never performs. `docs/modules/facade.md` states that split for consumers.

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
