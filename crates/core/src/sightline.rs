//! Rule sightline declarations — the vocabulary-free MECHANISM a rules crate uses to state, next to
//! the rule itself, WHERE that rule's trigger evidence can be witnessed at all.
//!
//! Several native rules read a structural fact that has exactly one producer (a single parser fills
//! it; every other parser leaves it empty). Such a rule is structurally silent — or, for a rule whose
//! trigger is liveness evidence, asserts from an empty channel — everywhere that producer never runs,
//! and its finding message says so only ON a finding, which the silent case by construction never
//! renders. Each affected rule already publishes this fact as a pinned prose sentence in its own
//! message module and catalog row; a [`RuleSightline`] is the MACHINE-READABLE half of the same
//! declaration, built by the owning rules crate from the very same pinned claim function and evidence
//! extension constant, so the two halves cannot drift apart. `zzop_engine::rule_sightlines` composes
//! every crate's declarations (the same aggregator shape as `register_all_native`), and the facade's
//! coverage query crosses them with a tree's extension mix to derive its CAPABILITY-kind blind-spot
//! cells — derived from rule metadata, never restated by hand.
//!
//! This module is the kernel's usual deal (see `registry::native_stub`): mechanism only, zero rule
//! vocabulary. No rule id, extension list, or claim sentence lives here — each declaration's data
//! lives in the crate that owns the rule, single-owner.

/// One rule's declared sightline: the file extensions in which its TRIGGER can be witnessed, because a
/// rule can only fire where its evidence extractor runs.
///
/// Direction of the claim, stated once so no consumer over-reads it: the extension set is an UPPER
/// BOUND, never a completeness claim. OUTSIDE the set the rule is structurally blind — its verdict
/// carries no information about those files. INSIDE the set coverage may still be partial (a sibling
/// consume path that leaves the fact unset, an idiom the recognizer does not model); nothing here says
/// otherwise, and [`Self::outside_meaning`] is worded per rule precisely so an asserting rule (one
/// that FLOODS rather than silences when blind) states its own inverse consequence.
#[derive(Debug, Clone)]
pub struct RuleSightline {
    /// The registered native analysis id, exactly as findings carry it.
    pub rule_id: &'static str,
    /// Extensions whose files the rule's evidence extractor covers — quoted from the owning crate's
    /// own evidence constant, never restated.
    pub trigger_extensions: &'static [&'static str],
    /// One sentence: what this rule's output means for files OUTSIDE [`Self::trigger_extensions`]
    /// (for a silent-when-blind rule, that zero findings there means not-analyzed, never clean; for an
    /// asserting rule, that its findings there are evidence-channel blindness, not a verdict). Built
    /// from the owning crate's pinned sightline claim, so it is a `String`, not a `&'static str`.
    pub outside_meaning: String,
    /// Which of the two blindness classes this rule belongs to — the DIRECTION of the claim a
    /// consumer must branch on when crossing this declaration with a tree's extension mix:
    /// - `false` (the SILENCE class): the rule goes quiet where blind — zero findings outside the
    ///   evidence channel means NOT ANALYZED. A consumer reports the extensions the rule cannot see.
    /// - `true` (the INVERSE class): the rule ASSERTS from an empty channel — findings outside the
    ///   evidence channel are evidence-channel blindness, not deadness. A consumer reports blindness
    ///   only when the whole channel is unfed (no witnessed extension present at all), because a fed
    ///   channel means the rule's evidence IS being gathered no matter what other files sit nearby.
    ///
    /// ONE flag rather than per-consumer heuristics, because the direction is a property of the
    /// rule's CLAIM — single-owner with the declaration, next to the pinned prose that words it.
    pub assert_when_blind: bool,
}
