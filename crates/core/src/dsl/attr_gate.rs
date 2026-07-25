//! Whole-tree ATTRIBUTE GATE for per-file DSL findings — the pass that lets a `line-scan` rule consume a
//! user DECLARATION instead of guessing the fact the declaration carries.
//!
//! ## What it is
//!
//! `LineScan::attr_present` / `attr_absent` / `require_attr_declared` (see those fields' docs for the
//! per-field contract) are evaluated HERE, never inside `crate::dsl::line_scan::eval_line_scan`. The
//! engine calls [`apply_attr_gates`] once per tree, after the per-file pass and before
//! `registry::merge_findings`.
//!
//! ONE call site, not two, and that is not an omission: Mode A (`analyze_envelope`) strips every
//! non-`SymbolScan`/`IoScan` rule out of a pack before evaluating it (`envelope::resolve::envelope_rule_pack`)
//! because an envelope carries no source text, so a `line-scan` rule cannot run — and cannot produce a
//! finding for this pass to gate, nor a suppressed count to disclose. Wiring it there would be provably
//! dead code. If Mode A ever gains text and starts running line-scan, this pass has to be wired into
//! `envelope::ingest` in the same position, and that is where the parity obligation reappears.
//!
//! ## Why not in the per-file evaluator: the cache would lie
//!
//! A per-file DSL finding is cached under `(content_hash, parser_fingerprint, scope,
//! ruleset_fingerprint)` — four ingredients, none of which is the `AttributeStore`. The store is an
//! ASSEMBLE-stage product (`AttributeStore::from_parts`: native producers' judgments merged with every
//! Mode-B adapter overlay), and overlays are deliberately outside the cache key because they are
//! re-applied every run. Evaluating an attribute gate inside the cached unit would therefore bake a
//! declaration into an entry that outlives it: edit the declaration, and warm files keep serving findings
//! computed against the old one, with nothing in the key to invalidate them.
//!
//! `io-scan` never had this problem — it already runs whole-tree at assemble, on the far side of the
//! cache. A post-filter puts `line-scan`'s gate on that same side without moving line-scan itself off the
//! fused per-file pass (which is where its performance comes from). The cache keeps storing UNGATED
//! findings, which is the honest thing for it to store: they are what the rule produced.
//!
//! ## What this placement costs, stated plainly
//!
//! A post-filter can only REMOVE findings, and only on a whole-file predicate. A gate that needed to
//! change WHICH lines match, or that varied within a file, could not live here — such a gate would have
//! to make the attribute set a cache-key ingredient first. Nothing shipped needs that; if something ever
//! does, this module is the wrong place for it, not the place to widen.
//!
//! ## §0 disclosure
//!
//! `require_attr_declared` makes a rule SILENT on an undeclared key. Silence that an agent has to notice
//! by itself is not disclosure (`output-philosophy` §0), so every suppressed rule that actually had
//! something to say reports one warning naming the rule, the key, the number of candidate sites dropped,
//! and how to declare it. A rule with nothing to say stays quiet: `0` findings there is a real `0`.

use std::collections::BTreeMap;

use crate::attributes::{attr_is_truthy, AttributeStore};
use crate::finding::Finding;
use crate::registry::is_enabled;
use crate::RuleConfig;

use super::def::{LineScan, Matcher, RulePackDef};

/// One enabled `line-scan` rule that declares at least one attribute gate, resolved to the shape the
/// filter below needs. Built once per rule, not per finding.
struct GatedRule<'a> {
    rule_id: String,
    m: &'a LineScan,
    /// `require_attr_declared` is set AND nothing in the store declares that key — so every finding this
    /// rule produced is dropped and the drop is disclosed.
    undeclared: Option<&'a str>,
}

/// Applies every enabled `line-scan` rule's attribute gates to `findings` IN PLACE, returning one
/// disclosure line per rule silenced by an undeclared `require_attr_declared` key (empty when none was).
///
/// `findings` may contain findings from any source — native analyses, io-scan, other packs. Only findings
/// whose `rule_id` matches a gated rule are ever touched, and the relative order of everything else is
/// preserved (`Vec::retain`), so this is a no-op for a config where no loaded rule declares a gate.
///
/// Rule ENABLEMENT is honored the same way the io-scan pass honors it (`is_enabled` at the pack id, then
/// at the `"{pack}/{rule}"` id): a disabled rule produced no findings to filter, and — more importantly —
/// must not emit a disclosure about not having run. It was turned off on purpose.
pub fn apply_attr_gates(
    packs: &[RulePackDef],
    rule_config: &RuleConfig,
    attrs: &AttributeStore,
    findings: &mut Vec<Finding>,
) -> Vec<String> {
    let gated: Vec<GatedRule> = packs
        .iter()
        .filter(|pack| is_enabled(rule_config, &pack.id))
        .flat_map(|pack| {
            pack.rules.iter().filter_map(move |rule| {
                let Matcher::LineScan(m) = &rule.matcher else {
                    return None;
                };
                if m.attr_present.is_none()
                    && m.attr_absent.is_none()
                    && m.require_attr_declared.is_none()
                {
                    return None;
                }
                let rule_id = format!("{}/{}", pack.id, rule.id);
                if !is_enabled(rule_config, &rule_id) {
                    return None;
                }
                let undeclared = m
                    .require_attr_declared
                    .as_deref()
                    .filter(|key| !attrs.declares(key));
                Some(GatedRule {
                    rule_id,
                    m,
                    undeclared,
                })
            })
        })
        .collect();
    if gated.is_empty() {
        return Vec::new();
    }

    // `rule_id -> (gate, dropped-for-undeclared count)`. The count is what separates "this rule had
    // nothing to say" from "this rule had something to say and could not judge it" — the only one of the
    // two worth a warning.
    let mut by_id: BTreeMap<&str, (&GatedRule, usize)> = gated
        .iter()
        .map(|g| (g.rule_id.as_str(), (g, 0usize)))
        .collect();

    findings.retain(|f| {
        let Some((gate, suppressed)) = by_id.get_mut(f.rule_id.as_str()) else {
            return true; // not a gated rule — untouched, including its position
        };
        if gate.undeclared.is_some() {
            *suppressed += 1;
            return false;
        }
        keeps_file(gate.m, attrs, &f.file)
    });

    by_id
        .values()
        .filter_map(|(gate, suppressed)| {
            let key = gate.undeclared?;
            (*suppressed > 0).then(|| undeclared_disclosure(&gate.rule_id, key, *suppressed))
        })
        .collect()
}

/// The per-file half of the gate: `attr_present` must resolve truthy for this file, `attr_absent` must
/// not. Both use `AttributeStore::path_attr` (exact `File` target beats the longest covering `PathScope`),
/// and both are plain conjunctive filters — a rule may set either, neither, or both.
fn keeps_file(m: &LineScan, attrs: &AttributeStore, file: &str) -> bool {
    if let Some(key) = &m.attr_present {
        if !attrs.path_attr(file, key).is_some_and(attr_is_truthy) {
            return false;
        }
    }
    if let Some(key) = &m.attr_absent {
        if attrs.path_attr(file, key).is_some_and(attr_is_truthy) {
            return false;
        }
    }
    true
}

/// The §0 disclosure line for a rule silenced by an undeclared key. Says four things, because an agent
/// reading it must be able to act without opening the source: what did not run, why, how much it was
/// holding, and the two ways out (declare the fact, or turn the rule off deliberately).
///
/// The declaration recipe names the USER-facing config key first and the embedder field in parentheses,
/// per `output-philosophy` §9 — `overlays` is what a `zzop.config.jsonc` author writes, `adapterOverlays`
/// is what an embedder passes. `key` is the pack's own vocabulary and appears verbatim: this module never
/// interprets it (kernel-vocab-free, same contract as `AttributeStore`).
fn undeclared_disclosure(rule_id: &str, key: &str, suppressed: usize) -> String {
    let sites = if suppressed == 1 {
        "1 candidate site".to_string()
    } else {
        format!("{suppressed} candidate sites")
    };
    format!(
        "rule \"{rule_id}\" did not run: it is gated on a declared `{key}` attribute and nothing in this \
         analysis declares one, so {sites} were found but left unjudged rather than guessed at. Declare \
         the fact with an overlay — in zzop.config.jsonc, `overlays: [\"./zzop-attributes.json\"]` \
         (embedders: `adapterOverlays`), whose file is a normalized-AST envelope carrying \
         `{{\"target\": {{\"pathScope\": {{\"prefix\": \"<dir>\"}}}}, \"key\": \"{key}\", \"value\": true}}` \
         in a file entry's `attributes` (an exact `{{\"file\": {{\"path\": \"<path>\"}}}}` target overrides a \
         covering scope; see examples/auth-overlay-adapter for a complete envelope). To leave it off \
         instead, say so: `rules: {{\"{rule_id}\": \"off\"}}` (embedders: `disabled_rules`)."
    )
}

#[cfg(test)]
mod tests;
