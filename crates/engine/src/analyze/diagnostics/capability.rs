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
                 Mode A/B adapter envelope via `overlays: [...]` in zzop.config.jsonc (embedders: \
                 `adapterOverlays`) — a partial Mode B envelope covering just the missing channel/files \
                 is enough to start (a tens-of-lines script; see the examples/ adapters). The contract \
                 ships inside the binary: MCP hosts print it with `zzop-mcp contract envelope-guide` \
                 (machine-checkable schema: `zzop-mcp contract envelope-schema`); repo users, see \
                 docs/NORMALIZED_AST.md."
            )
        })
        .collect()
}
