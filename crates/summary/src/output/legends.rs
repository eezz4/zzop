//! THE LEGEND FOLD (2026-09-01) — the second application of the rule `output::disclosure` wrote down,
//! to the RUN-INVARIANT legends that were still shipping their full text on every call.
//!
//! # What it does, and why it is allowed to
//! Some channels of an analyze-shaped reply carry a VOCABULARY rather than this run's own numbers,
//! and those are what fold. [`folded`] is the population; count it there rather than here, with the
//! field anchored to its own indentation so the counting command does not count itself:
//! `grep -c '^            key: "' crates/summary/src/output/legends.rs` -> 8 on 2026-09-14.
//! (An unanchored `grep -c 'key: "'` returns 9. It matches this very line, which is the same way the
//! repo's deadline scan once grew 35 -> 38 -> 40 by being explained three times.)
//!
//! 🔴 This paragraph used to open "the FOUR run-invariant legends" and name them. The number was right
//! on 2026-09-01 and wrong by 2026-09-14 — twice over: two more had been folded in between, and two
//! that qualified from the start (`architecture.topRecommendationMeaning`, born 2026-08-20, and
//! `architecture.criticalTopMeaning`) had never been in the population at all. They were in neither the
//! fold list NOR the "deliberately not folded" list below, which is the state a hand-picked population
//! decays into. Ledger V240 carries the guard that would have caught it; this line carries the reason
//! the count is a command now.
//!
//! 📏 The original four, on the trees they were measured on (koel, cal.com, nocodb, fe-axios), were
//! BYTE-IDENTICAL across every tree — 4,416 + 1,022 + 1,962 + 1,074 = 8,474 bytes of prose repeated on
//! every call, for an output whose primary reader is an AI agent that calls repeatedly. The pair added
//! on 2026-09-14 removed a further 1,081 bytes from every reply — the SAME number on all four trees
//! measured, which is what fixed prose looks like (fastapi 35,802 -> 34,721; express 30,568 -> 29,487;
//! gin 34,682 -> 33,601; aspnetcore 63,436 -> 62,355).
//!
//! `disclosure`'s own note already states the rule this module generalizes: *"Identical every run, so
//! the full text ships once, not per call."* What leaves is the invariant prose. What stays, in every
//! case, is THIS RUN's measurement — the `coverageGaps.extensions` rows and their `basis`, the
//! `byRule` counts, the `nativeAnalyses` lists, the `packsLoaded` rows — plus a short note carrying the
//! one reading a reader must not get wrong, so nothing here turns a present disclosure into an absent
//! one.
//!
//! # What is deliberately NOT folded, and why each
//! * `architecture.painMeaning` / `painByAxis` — staying, and since 2026-09-11 it is no longer
//!   INVARIANT either, so both reasons now apply. Output principle §1.5 was always the first: a
//!   SYNTHESIZED NUMBER ships with the statement of what it measures, and folding it would reopen that
//!   decision rather than apply this one. The second arrived with
//!   `scores.excludeTestFilesFromFileMetrics`, which RE-BASES `pain` on the same bytes: measured on
//!   the fixed `corpus/frameworks/express` checkout, 30.0 with the key off against 11.5 with it on,
//!   while `findings.total` stayed 58 and `byRule` stayed byte-identical (re-measured 2026-09-23; 56 until
//!   then — the invariant is the claim, not the count, and the recount lives once in `crates/facade/src/request/scores.rs`). Reproduce: add
//!   `"scores": {"excludeTestFilesFromFileMetrics": true}` to a copy of that tree's
//!   `zzop.config.jsonc` beside the tree, and diff `zzop analyze --config <copy> --limit 0`.
//!   `painMeaning` now carries a POPULATION clause computed per run from `health.testFilesExcluded`,
//!   so it has no "ships once" to appeal to — the same disqualification the cross-layer
//!   `nativeAnalysesMeaning` below already carries. Cost of the clause, measured on that same tree:
//!   `painMeaning` 1344 -> 1610 bytes with the key off, 1836 with it on.
//! * The cross-layer join's `nativeAnalysesMeaning` (`crate::cross::native_analyses`) — NOT invariant.
//!   Its `zeroInCrossLayerFindings` sentence is computed per run from the findings' own
//!   `data.replaces`. A run-varying sentence has no "ships once" to appeal to.
//! * `findings.ruleMessagesMeaning` / `ruleMessageTemplatesMeaning` — invariant, but their byte cost is
//!   an INPUT to the message fold's price gate (`rule_prose::cost`), so shrinking them changes which
//!   rules fold. That is a detection-adjacent change and belongs in its own batch, not smuggled in
//!   here.
//! * `warnings` — the run-VARYING channel `disclosure`'s doc already carves out, measured here at
//!   1,645 / 6,054 / 14,208 / 19,580 bytes on the same four trees. Nothing about it repeats, so there
//!   is no copy to remove; capping it instead is the direction output principle §13 deleted (the
//!   bucket-key cap) and `product/persona.md` refuses for the targeting surfaces.
//!
//! # The one property that keeps it honest
//! [`FOLDED`] is the single list. The reply reads it to build its pointers and the contract document
//! reads it to render the text, so a legend cannot be folded out of one without appearing in the
//! other. `crates/summary/tests/legend_fold.rs` walks that list from both ends: every full text is
//! ABSENT from a shaped reply and PRESENT in the document the reply points at.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::contracts::{REPLY_LEGENDS_CONTRACT_NAME, URI_PREFIX};

/// One folded legend: where it rides on the wire, the short note that stays there, and the full text
/// that moved to the contract document.
pub(crate) struct FoldedLegend {
    /// The reply path, spelled the way a reader would: this is the heading the document is looked up
    /// by, so it must be the string someone reading the reply already has in hand.
    pub(crate) key: &'static str,
    /// The note that stays on the wire — the reading a reader must not get wrong, without the
    /// vocabulary behind it.
    pub(crate) note: fn() -> String,
    /// The full text, as `(sub-key, body)` pairs. An empty sub-key means the legend is one unsectioned
    /// string (`coverageGaps.meaning`, `byRuleMeaning`); a non-empty one is a key of the legend object
    /// (`registered`, `filesInScope`, ...).
    pub(crate) full: fn() -> Vec<(&'static str, String)>,
}

/// Every legend this module folds, in the order the document presents them. THE list — both directions
/// read it, which is what makes "folded out of the reply" and "present in the document" one fact
/// instead of two that drift.
pub(crate) fn folded() -> Vec<FoldedLegend> {
    vec![
        FoldedLegend {
            key: "coverageGaps.meaning",
            note: coverage_gaps_note,
            full: || vec![("", crate::analyze::coverage_gaps_meaning())],
        },
        FoldedLegend {
            key: "module_map.meaning",
            note: module_map_note,
            full: || vec![("", crate::module_map::MEANING.to_string())],
        },
        FoldedLegend {
            key: "findings.byRuleMeaning",
            note: by_rule_note,
            full: || vec![("", super::by_rule_legend::BY_RULE_MEANING.to_string())],
        },
        FoldedLegend {
            key: "findings.shownMeaning",
            note: shown_note,
            full: || vec![("", super::shown_legend::SHOWN_MEANING.to_string())],
        },
        FoldedLegend {
            key: "findings.byDirectory.meaning",
            note: by_directory_note,
            full: || vec![("", super::by_directory::BY_DIRECTORY_MEANING.to_string())],
        },
        FoldedLegend {
            key: "nativeAnalysesMeaning",
            note: native_analyses_note,
            full: || flatten(zzop_facade::native_analyses_legend()),
        },
        FoldedLegend {
            key: "packsLoadedMeaning",
            note: packs_loaded_note,
            full: || flatten(zzop_facade::packs_loaded_legend()),
        },
        FoldedLegend {
            key: "architecture.topRecommendationMeaning",
            note: top_recommendation_note,
            full: || {
                vec![(
                    "",
                    super::architecture_legends::TOP_RECOMMENDATION_MEANING.to_string(),
                )]
            },
        },
        FoldedLegend {
            key: "architecture.criticalTopMeaning",
            note: critical_top_note,
            full: || {
                vec![(
                    "",
                    super::architecture_legends::CRITICAL_TOP_MEANING.to_string(),
                )]
            },
        },
    ]
}

fn flatten(legend: BTreeMap<&'static str, &'static str>) -> Vec<(&'static str, String)> {
    legend
        .into_iter()
        .map(|(k, v)| (k, v.to_string()))
        .collect()
}

/// The clause a folded STRING ends with. A string-valued channel has no sibling slot for a machine
/// pointer, so the two host dialects are spelled inside the sentence — both of them, in the order and
/// shape this repo's other two-dialect messages use, because this exact text reaches a CLI reader with
/// argv and an MCP client with none.
///
/// The names are LITERAL rather than interpolated, and that is a guard requirement rather than a style
/// choice: contract 16's twin resolution (`crates/engine/tests/rule_contracts/host_vocabulary.rs`)
/// reads the document name that FOLLOWS the CLI spelling in the SOURCE text, so an interpolated name
/// resolves to no document at all and the sentence is scored as a CLI-only spelling with no MCP twin.
/// The copy that buys is sealed rather than trusted — [`tests`] asserts both literals against
/// [`REPLY_LEGENDS_CONTRACT_NAME`] and [`URI_PREFIX`], which is the same trade
/// `warnings::rule_filter`'s two-dialect prescription already makes.
const INLINE_POINTER: &str = "This vocabulary is identical on every run, so its full text ships once \
     rather than on every call: read it under this key's own heading in the reply-legends document — \
     MCP resource `zzop://contract/reply-legends` on MCP hosts (`zzop contract reply-legends` with \
     the CLI binary).";

fn inline_pointer() -> String {
    INLINE_POINTER.to_string()
}

/// The pointer half of a folded OBJECT legend: `{note, resource, command}`, the shape
/// [`super::disclosure::fold`] established. Machine-readable pointer keys, and a note that says to read
/// them — the same division that fold uses, so a reader who has seen one has seen both.
fn object_pointer(note: String) -> Value {
    json!({
        "note": format!(
            "{note} This vocabulary is identical on every run, so its full text ships once rather than \
             on every call — read it with the command/resource below, under this key's own heading."
        ),
        "resource": format!("{URI_PREFIX}{REPLY_LEGENDS_CONTRACT_NAME}"),
        "command": format!("zzop contract {REPLY_LEGENDS_CONTRACT_NAME}"),
    })
}

pub(crate) fn folded_string(key: &str) -> String {
    folded()
        .into_iter()
        .find(|l| l.key == key)
        .map(|l| (l.note)())
        // Unreachable through the callers below, which pass the same literals `folded()` holds; the
        // fallback is a pointer rather than a panic because a reply path must not abort, and a bare
        // pointer is still true.
        .unwrap_or_else(inline_pointer)
}

/// The folded value for a legend that ships as a JSON OBJECT.
pub(crate) fn folded_object(key: &str) -> Value {
    object_pointer(
        folded()
            .into_iter()
            .find(|l| l.key == key)
            .map(|l| (l.note)())
            .unwrap_or_default(),
    )
}

/// The `reply-legends` contract document — every folded legend's FULL text, rendered from the same
/// [`folded`] list the reply builds its pointers from.
///
/// Rendered rather than embedded, for `disclosure-classes`' reason one step further: two of these
/// bodies are `format!`ed from live constants (a share floor, a registry) and none of them has a file
/// to `include_str!`. A committed copy would be the second hand-maintained text the fold is not
/// allowed to have.
pub fn contract_text() -> String {
    let mut out = String::from(
        "# Reply legends\n\nThe run-INVARIANT vocabulary of a shaped zzop reply \
         (analyze-shaped, and since 2026-09-15 the module map too). Every \
         section below is the full text of one reply key; that key still ships on every reply, \
         carrying this run's measurement plus a short note, and points here for the rest.\n\nThese \
         sentences are byte-identical on every run and for every repository, which is why they ship \
         once here instead of on every call. Nothing in this document is about YOUR tree — the \
         numbers, lists and rows they explain are in the reply itself.\n",
    );
    for legend in folded() {
        out.push_str(&format!("\n## `{}`\n\n", legend.key));
        for (sub, body) in (legend.full)() {
            if sub.is_empty() {
                out.push_str(&format!("{body}\n"));
            } else {
                out.push_str(&format!("### `{sub}`\n\n{body}\n\n"));
            }
        }
    }
    out
}

/// The WORDS. Split out for the 400-line file cap; see that module's own doc for the seam.
mod notes;

pub(crate) use notes::{
    by_directory_note, by_rule_note, coverage_gaps_note, critical_top_note, module_map_note,
    native_analyses_note, packs_loaded_note, shown_note, top_recommendation_note,
};

#[cfg(test)]
mod tests;
