//! The unknown-config-id self-reports — the four checks in this crate that are NOT about degenerate
//! output.
//!
//! Every other diagnostic in `diagnostics.rs` answers *"does this analysis look empty?"*. These four
//! answer *"did a config entry the caller wrote do nothing (or, for one of them, everything)?"*. They
//! were split out of `build_diagnostics` when that file crossed the 300-line cap, and the family is
//! the right seam because they share a shape: sort, dedup, name the offending ids, name the config
//! dialect AND the embedder spelling, say what the entry failed to do, and — since 2026-08-13 — hand
//! over the same two readings of WHY ([`exported_pack_reading`], one sentence for all four, so the
//! next channel to be fixed cannot leave a sibling behind the way this one was left).
//!
//! ## Two channels, on purpose
//! Three of the four ride `config_warnings`; `unknown_suppression_rule_ids` rides `warnings`. That
//! split is deliberate and predates this file (see `AnalysisDiagnostics::config_warnings`'s doc), so
//! the two functions here are named for the channel rather than the knob — a caller wiring these into
//! `AnalyzeOutput` cannot mix them up, and the asymmetry stays visible instead of hiding inside one
//! function that returns a pair.
//!
//! It also cost someone real time: a 2026-08-11 audit of stale rule ids read `configWarnings` only and
//! concluded the `exclude` channel was silent. It is not — it is one field over.

use super::{entry_count, DiagnosticsInput};

/// The SECOND reading every report in this file owes its reader, spelled once.
///
/// ## Why one sentence is not enough (2026-08-13)
///
/// All four reports used to say "check for a typo" and stop. That is the EXACT WRONG CONCLUSION for a
/// population this release created: v0.30.0 moved four packs out of the bundle into `examples/packs/`,
/// and every one of their rule ids is a REAL id that a config may legitimately name — measured on a
/// v0.29.0 config carried forward unchanged, `rules: { "typescript/no-explicit-any": "off" }` was
/// answered with "check for a typo". The id is not misspelled; the pack is simply not loaded.
///
/// The judgment was already made in this build, one channel over: `zzop_summary`'s findings-view `rule`
/// filter warning gives both readings and names this file as the sibling still getting it wrong. This
/// is that clause ported, not a second invention — the two channels answer the same reader about the
/// same id space and must not disagree about what an unmatched id means.
///
/// ## Why the CONTRACT RESOURCE and not `examples/packs/`
///
/// That directory is a REPO path. Whoever receives this warning is by construction someone whose build
/// does not carry the pack — an npm or `.mcpb` install has no checkout to look in. The retrieval path
/// that exists for them is the embedded contract document, which every host serves.
///
/// Both host dialects are spelled, the engine's standing practice for text that reaches CLI and MCP
/// readers alike (`zzop contract <doc>` is no subcommand an MCP host can type, and a bare
/// `zzop://contract/<doc>` URI is nothing a CLI user can fetch). Pinned by `host_vocabulary` contracts
/// 15/16, which fail this file if either dialect ever loses its twin.
///
/// ## Why a function and not a `const`
///
/// The same reason `sorted` and `entry_count` are functions: this module's way of spelling a shared
/// piece once is a private helper, not a declared name. It also keeps a user-facing SENTENCE out of
/// `scripts/policy-census.txt`, whose subject is policy-shaped constant NAMES under the rule and
/// extraction crates — a message is neither a policy value nor vocabulary, and a census row for it
/// would be a triage moment with nothing to triage.
fn exported_pack_reading() -> &'static str {
    "Two readings, and they need different fixes: the entry may be misspelled, or it may name a pack \
     this build SHIPS BUT DOES NOT LOAD — an exported pack is a real id, not a spelling mistake. Every \
     exported pack is retrievable from this build and starts matching once its file is in the tree: \
     MCP resource `zzop://contract/example-pack-<stem>` on MCP hosts (`zzop contract \
     example-pack-<stem>` with the CLI binary; the contract index lists one entry per exported pack), \
     saved under `<tree>/zzop/rules/` or in a directory named by `packs.extraDirs` — that key \
     REPLACES the `zzop/rules/` default rather than adding to it, so a tree already using it must \
     name this directory too."
}

/// The three unknown-id reports that belong on the caller's config-warnings channel.
pub(super) fn config_channel_reports(i: &DiagnosticsInput) -> Vec<String> {
    let mut out = Vec::new();
    let reading = exported_pack_reading();

    // Unlike the degenerate-output checks, this one flags a config entry that had NO effect at all (a
    // typo'd/stale `disabled_rules` id), which is otherwise indistinguishable from a working exclusion
    // (see `unknown_disabled_rule_ids`'s doc).
    if !i.unknown_disabled_rule_ids.is_empty() {
        let ids = sorted(&i.unknown_disabled_rule_ids);
        out.push(format!(
            "disabled rules have {} matching no known rule id: {} — these did NOT disable anything. {reading} (a valid id is a bare pack id, a native analysis id, or a \"<pack>/<rule>\" id; config dialect `rules: {{ \"<id>\": \"off\" }}` for a rule id, or `packs.disabled` for a bare pack id; embedders: `disabledRules`).",
            entry_count(ids.len()),
            ids.join(", ")
        ));
    }

    // Same "config entry had NO effect at all" class as `unknown_disabled_rule_ids` above, over
    // `severity_overrides` instead — see that field's doc and `unknown_severity_override_ids`'s doc for
    // why the valid-id enumeration named here (no bare pack id) differs from the disabled-rules one.
    if !i.unknown_severity_override_ids.is_empty() {
        let ids = sorted(&i.unknown_severity_override_ids);
        out.push(format!(
            "severity overrides have {} matching no known rule id: {} — these did NOT remap any finding's severity. {reading} (a valid id is a native analysis id or a \"<pack>/<rule>\" id; config dialect `rules: {{ \"<id>\": \"<severity>\" }}`, embedders: `severityOverrides`).",
            entry_count(ids.len()),
            ids.join(", ")
        ));
    }

    // The pack ALLOWLIST's unknown-id report, and the one member of this family whose failure inverts:
    // the three above say "this did nothing", this one has to say "this turned everything off", because
    // `is_pack_enabled` treats a non-empty allowlist naming no loaded pack as an allowlist admitting no
    // pack at all. Landed 2026-08-11; before it, that state had no wire receipt in any field.
    if !i.unknown_only_pack_ids.is_empty() {
        let ids = sorted(&i.unknown_only_pack_ids);
        // The two wordings are not stylistic. A partial typo leaves the run working, and borrowing the
        // total-typo sentence there would teach the reader to distrust it in the case that matters.
        let consequence = if i.only_packs_matched_nothing {
            " — NO entry named a loaded pack, so the allowlist admitted NO pack and EVERY DSL rule was gated off for this run (native analyses are not packs and still ran). This is why the report may look empty"
        } else {
            " — these named no loaded pack and contributed nothing to the allowlist (the entries that DID match still gate the run)"
        };
        out.push(format!(
            "pack allowlist has {} matching no loaded pack id: {}{consequence}. {reading} (a valid entry is a BARE pack id — not a \"<pack>/<rule>\" id and not a native analysis id; config dialect `packs.only`, embedders: `packsOnly`; `packsLoaded` in this reply lists the ids that exist).",
            entry_count(ids.len()),
            ids.join(", ")
        ));
    }

    out
}

/// The one member of the family that rides `warnings` instead — unchanged by the 2026-07-17 split that
/// moved its two siblings, and deliberately left there.
///
/// Orthogonal to the unmatched-path/glob-filter warning (`unmatched_suppression_warnings` in
/// `zzop-engine`): that one flags a *filter* matching no scanned file, this one flags the *rule id*
/// itself matching nothing. A single suppression entry can trigger both — a typo'd rule id AND a typo'd
/// filter are two separate mistakes on the same entry.
pub(super) fn warning_channel_reports(i: &DiagnosticsInput) -> Vec<String> {
    if i.unknown_suppression_rule_ids.is_empty() {
        return Vec::new();
    }
    let ids = sorted(&i.unknown_suppression_rule_ids);
    let reading = exported_pack_reading();
    vec![format!(
        "suppressions have {} whose rule matches no known rule id: {} — these did NOT suppress anything. {reading} (a valid id is a native analysis id or a \"<pack>/<rule>\" id; config dialect `rules: {{ \"<id>\": {{ \"exclude\": [...] }} }}`, embedders: `suppressions`).",
        entry_count(ids.len()),
        ids.join(", ")
    )]
}

/// Sort + dedup, spelled once. Every report in this file needs both: the warning text is compared
/// byte-for-byte by tests and by users diffing two runs, so an input-order-dependent sentence would
/// make an identical config look like a changed one.
fn sorted(ids: &[String]) -> Vec<String> {
    let mut out = ids.to_vec();
    out.sort();
    out.dedup();
    out
}
