//! [`RuleAxis`] — the defect/opinion axis a DSL rule declares.
//!
//! Its own file since 2026-08-12, when the doc explaining WHY the axis exists pushed the parent
//! `def/mod.rs` over the repo line cap. The split is along the seam the module already had: the parent
//! carries the pack/rule ENVELOPE and fragment expansion, and this carries the one field whose meaning
//! is an argument rather than a shape.

use serde::{Deserialize, Serialize};

/// What KIND of claim a rule makes — the axis a reader needs before deciding whether a finding is a
/// bug report or a style note.
///
/// Added 2026-08-12 after a user pointed at the gap in one sentence: *"barrel-export discipline is not
/// a rule, it's a house convention."* The measurement behind it, taken that day on a 132-rule bundle:
/// 17 rules flagged a shape a competent team can deliberately choose in production and be right about,
/// and nothing in the pack format said so. Sixteen of those 17 have since moved to `examples/packs/` —
/// declaring the axis is what made them findable as a set, so the census that motivated the field is
/// not a number to maintain here. Recount either side with
/// `grep -c '"axis": "opinion"' rules/dsl/*/*.json examples/packs/*.json`.
///
/// `severity` does NOT stand in for this and was measured not to — the `info` band holds
/// `mutating-route-no-auth` and `ws-no-auth` beside `destructive-migration`, because severity encodes
/// CONFIDENCE x BLAST RADIUS, a different question with a different answer.
///
/// The sibling axis on the scores side is `zzop_metrics::HealthAxis`, and the two are the same concept
/// in the two places zzop makes a judgment. They are deliberately NOT one shared type: this one is part
/// of the DSL pack format a third party authors against, that one is an internal weight-table column,
/// and fusing them would put a public authoring contract and an engine detail in one place.
///
/// ## Why this is not on [`crate::Finding`]
///
/// A finding's axis would be the more useful surface, and it is deliberately absent: NATIVE analyses
/// (`circular`, `cross-layer/*`, `schema/*`) register through `RuleRegistry`, which holds ids and
/// nothing else, so they cannot declare one. An axis that rode only DSL findings would report a
/// complete-looking split over a subset of the findings — the exact partial truth this repo refuses
/// elsewhere. Closing it means giving the native registry per-analysis metadata first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleAxis {
    /// Wrong regardless of house taste — nobody deliberately ships it.
    ///
    /// **The default**, for two reasons. A pack written before this field (and every third-party pack)
    /// keeps loading unchanged. And a rule that FORGOT to declare should read as the stronger claim and
    /// get argued down in review, never quietly demoted to "just an opinion". Packs SHIPPED FROM THIS
    /// REPO may not rely on it, `rules/dsl/` and `examples/packs/` alike:
    /// `rule_contracts::rule_axis::every_shipped_rule_declares_its_axis` reads the pack
    /// JSON TEXT, because a serde default is invisible after parsing — only the text can tell "declared
    /// defect" from "said nothing". Exporting a pack was a way to stop declaring until 2026-08-12,
    /// and twelve rules that took it were described as axis-less by two documents in two days when they
    /// were in fact loading as `defect`.
    #[default]
    Defect,
    /// A preference about how code is arranged or written. A project that deliberately does the
    /// opposite is not wrong; it disagrees. Every rule carrying this must be able to name the case in
    /// which the flagged shape is the RIGHT call, and its `message` should say so.
    Opinion,
}
