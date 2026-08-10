//! The two config-derived self-reports built on the [`DslScope`](super::DslScope) census — the
//! tree-wide "not one loaded rule applies" note and the per-pack "these packs matched nothing, and
//! here is the lever" advice. Split out of `pack_scope` on the file-line cap; the census itself and
//! the [`pack_scope_warnings`](super::pack_scope_warnings) entry point that chains these stay in the
//! parent (one entry point, same census, fixed order — that contract is documented there).

use super::DslScope;

/// Capability self-report (D16): packs loaded (`config.packs` non-empty) but not a SINGLE loaded rule's
/// `file_pattern` matches any file this tree actually analyzed — e.g. a Go-only tree loaded against
/// TS/Python-oriented packs. Without this, "112 rules loaded, 0 findings" is undiagnosable: it reads
/// identically to "112 rules loaded, ran, tree is genuinely clean". This distinguishes "no applicable
/// rules" from "clean" — native structural/whole-graph analyses still ran regardless (they are not
/// `file_pattern`-gated), so this is purely a DSL-coverage disclosure. `scope` must be the
/// [`compute_dsl_scope`](super::compute_dsl_scope) census over the SAME `packs` slice (the caller
/// computes it once and shares it with `AnalyzeOutput::packs_loaded`'s per-pack `files_in_scope`).
pub(super) fn no_applicable_dsl_rule_warning(
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
/// path candidacy computed by [`compute_dsl_scope`](super::compute_dsl_scope) BEFORE any content is
/// read — it is NOT "you have no redis", NOT "this pack is useless here", and NOT evidence that a
/// stack is absent. A tree gains a matching path the moment someone adds one file, and the pack is
/// back in scope with no edit at all. So the message states the path fact, names the lever, says what
/// dropping a pack buys (the rule evaluations it performs), and stops. The engine deliberately does
/// NOT act on this itself: an engine that inspected a tree for evidence of a stack and skipped packs
/// on its own would be guessing at what the caller can simply declare, and a wrong evidence vocabulary
/// makes SECURITY rules silently not run.
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
pub(super) fn zero_scope_packs_warning(
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
