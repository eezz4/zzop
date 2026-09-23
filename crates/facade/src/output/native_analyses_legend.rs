//! `nativeAnalysesMeaning` — the legend for `nativeAnalyses`, written to the same contract as its
//! older sibling [`super::packs_legend`]: a machine field ships beside a sentence that says what it
//! measures, and the sentence names the reader's next ACTION, because the whole point of splitting
//! the causes apart is that the actions differ.
//!
//! # Why a sibling key, and why unconditional
//! Sibling for the reason `packsLoadedMeaning` is one — the alternative is repeating a legend inside
//! rows — and the same shape `painMeaning`/`criticalTopMeaning`/`scoreMeanings`/`dispatchMeaning`
//! already use. Unconditional, unlike `packsLoadedMeaning`, because its subject is unconditional:
//! `packsLoaded` can legitimately be empty (a run with no DSL packs has no roster to explain), while
//! every build registers native analyses and every per-tree reply therefore carries the fact this
//! legend explains.
//!
//! # The sentence a reader was missing
//! Before this, a reply's `findings.byRule` had no `cross-layer/*` key on a corpus where the same
//! config, run through the cross-layer join, produced 975 of them. Nothing in the reply distinguished
//! that from a tree with no cross-layer defects. The legend's job is to make the distinction sayable
//! in the reader's own words rather than inferable from a field name.
//!
//! # Two of these sentences are LANE-INVARIANT and the join lane reads them from here
//! [`NATIVE_ANALYSES_REGISTERED_MEANING`] and [`NATIVE_ANALYSES_DISABLED_MEANING`] describe the BUILD
//! and the GATE, which mean the same thing in a per-tree reply and in the cross-layer join reply, so
//! `zzop_summary`'s join legend composes them instead of restating them (it appends one clause to the
//! second, naming WHOSE config did the disabling). The other three sentences deliberately do NOT travel:
//! in a join reply `crossLayerFindings` is the channel the reader is looking AT, so "run the join to
//! see these" would tell them to run what they just ran. A key can keep its name across two lanes
//! while the ACTION it licenses inverts — which is why the legend, not the field name, is what forks.
//!
//! [`NATIVE_ANALYSES_SHIPPED_OFF_MEANING`] is the third non-traveller, and for a third reason: the join
//! reply has no `shippedOff` key, and giving it one would require inventing a fact. The join gate is
//! EXCLUDE-ONLY (`merge_config::union_configs`) — it can turn an analysis off for the run, never on — so
//! "did this analysis run" is settled per TREE, and a config that opted in for one tree and not another
//! has no single honest entry: a union would call it skipped where it ran, an intersection would call it
//! run where it was skipped. The join legend's `everythingElse` sentence therefore names the shipped-off
//! case and sends the reader to the per-tree roster, rather than a key here answering for it.
//!
//! Two things this split fixes rather than complicates. The join's `disabled` sentence gets STRICTER:
//! that list is now only what a config chose, never contaminated by a project default the reader would
//! have been told they picked. And the join's cross-layer POPULATION is safe by construction — each
//! tree's `reportedInCrossLayerFindings` is already filtered through `is_enabled`, the same gate that
//! holds the shipped-off set, so a shipped-off cross-layer id would drop out of every tree's list and
//! out of the join's union of them. That safety is a property of the current set, not of the design, so
//! it is asserted where it can break: `rule_contracts::native_analyses` refuses a shipped-off id that is
//! also a cross-layer one.
//!
//! # The residual sentence used to contradict its own neighbour
//! [`RESIDUAL`] claimed that any registered analysis in neither list "ran, and its absence from
//! `findings` is a measured zero". [`NATIVE_ANALYSES_REGISTERED_MEANING`], two keys away, already said
//! that some registrations gate score computations emitting no finding at all, and that others are
//! umbrella ids whose findings arrive under finer `schema/<label>` ids. For those, absence from
//! `findings` is structural and measures nothing — so one sentence in this object was evidence against
//! another, and a reader who believed the residual one would have read "no defect found" off an id
//! that cannot report a defect under its own name. The carve-out below NAMES the two classes and
//! points at `registered` for their description rather than repeating it: two copies of that
//! description would disagree the first time a registration class is added.

use std::collections::BTreeMap;

/// What `registered` counts. LANE-INVARIANT — a build fact, identical on every reply shape, and read
/// by `zzop_summary`'s cross-lane legend so the two replies cannot drift about their own denominator.
///
/// # The two excepted classes are NAMED, never COUNTED — and that is not a style choice
/// This sentence used to say "five of them ... and two are ...". Both numbers were right the day they
/// were written and neither is derivable from here: the score-gating ids are
/// `zzop_metrics::register_native_analyses`'s (visible to this crate, but only at runtime, and this
/// value is a `const` two other crates consume by identity), and the umbrella ids are
/// `zzop_rules_schema`'s two family gates — a crate this one has no dependency edge to, and which
/// exports no constant for "the umbrella subset" even to a crate that does. So the counts were a hand
/// stamp with no source to fall back on, and the residual sentence below then made them load-bearing
/// by deferring to this one for the description of the very classes it carves out. A sixth
/// score-gating id would have left a false number here and a carve-out pointing AT the false number.
///
/// What the reader can act on is WHICH classes exist, not how many ids are in them: the action is
/// "look under `schema/<label>` rather than the umbrella id", and no count changes it. Naming without
/// counting makes the sentence true for every future registration in either class, which is the same
/// repair shape as deriving — the failure mode is gone rather than watched. The test module below
/// keeps a count from growing back.
pub const NATIVE_ANALYSES_REGISTERED_MEANING: &str =
    "how many native analyses this build registers — the DENOMINATOR the \
lists below are read against. It counts the GATE id space (what `rules`/`disabledRules` and \
`ruleOverridesApplied` name), not the set of ids a finding can carry: some of them gate score \
computations that emit no finding, and some are umbrella registrations whose findings carry finer \
`schema/<label>` ids. The MEMBERS of that denominator are not in this reply: \
the `rule-catalog` contract document lists every id this build ships.";

/// What `disabled` means. LANE-INVARIANT as far as it goes — the join reply appends one clause about
/// WHOSE config did the disabling (any tree's, exclude-only) rather than restating this sentence.
///
/// # Why the sentence no longer says "were NOT evaluated" flatly (2026-09-13, ledger V198)
///
/// It was false for a case the engine has documented all along and this legend did not:
/// `crates/engine/src/analyze/assemble/metrics.rs` computes `scores` whenever EITHER `scores` or
/// `health` is enabled, because `health` is computed from it — "it just cannot also skip the
/// computation while `health` still needs it", and the run records the work under the `scores` timing
/// key because it really ran. Measured on a fixture with `rules: {"scores": "off"}`: the whole reply is
/// identical to the run without it apart from the two `disabled` lists, `architecture.pain` included.
/// So the reply asserted a thing had not been evaluated while shipping a number derived from it.
///
/// The repair is the sentence, not the behaviour: suppressing a field a dependent analysis still needs
/// would trade a false claim for a missing answer. What the wire owes the reader is which of the two
/// states it is in, and the general shape of that — an id may be withheld without being skipped —
/// is what this now says. WHICH ids have such a dependent is per-id knowledge and belongs where per-id
/// knowledge lives (`zzop explain <id>`), not in a hand-kept list on the wire that would rot the first
/// time the dependency graph moved.
pub const NATIVE_ANALYSES_DISABLED_MEANING: &str =
    "native analyses this run's config switched off by id (config `rules`, \
embedders: `disabledRules`), sorted. Their OUTPUT is withheld, so their absence from the findings map \
means not reported — never analyzed-and-clean. Withheld is not always unevaluated: where a disabled \
analysis feeds another one that is still enabled, it is computed anyway and only its own output is \
dropped, which is why turning one off does not always make a reply smaller or change a number derived \
from it. To get a verdict from one, stop disabling it.";

/// What `shippedOff` means. The third cause an analysis can be missing from `findings`, and the one
/// the reader did not choose -- so the sentence has to say who chose it, why, and how to undo it.
///
/// # Provenance of the 61.7%, and why it is HERE rather than in the string (2026-09-13, ledger V178)
///
/// The wire text carries a bare percentage. A measurement with no date and no population is the shape
/// that got a TRUE number reported as stale earlier in this same review round (V158) -- the defence
/// has to exist somewhere, and the somewhere is not the wire.
///
/// Owner of the derivation: `zzop_rules_graph`'s `shipped_off` doc, measured 2026-09-03 (U151-m0) as
/// **2,570 of 4,164 firings across the six dogfood trees** (`unimported-export` 2,071 +
/// `dead-candidates` 498 + `unreachable` 1). It is a dated SNAPSHOT, not a live figure, and it is not
/// recountable as written: that corpus has since grown past six trees, so "the six" no longer names a
/// set anyone can rebuild. Re-deriving it means choosing a NEW population and saying so.
///
/// What keeps the copies honest meanwhile is `scripts/check-shipped-off-sync.sh`, which anchors every
/// surface carrying this claim -- this constant included, as of the same ledger row -- on the `61.7%`
/// marker, so the number cannot move in one place only.
///
/// Deliberately NOT added to the string: the date would be wire bytes, and the 2026-09-12 product
/// decision on reply size runs the other way. Provenance belongs to the reader of the source, the
/// claim belongs to the reader of the reply.
pub const NATIVE_ANALYSES_SHIPPED_OFF_MEANING: &str =
    "native analyses THIS BUILD ships switched off, which your config did not turn on, sorted. \
They were NOT evaluated, exactly like `disabled` above -- the difference is whose choice it was, and \
that difference is why they are two lists rather than one. What ships off is unused-code hygiene \
(`unimported-export`, `dead-candidates`, `unreachable`): measured across a six-repository corpus they \
were 61.7% of every finding reported, all three report `info` rather than claiming a defect, and a \
first run whose top half is hygiene buries the findings that do claim one. Nothing is removed from \
the build. To get a verdict from one, name it in `rules` with a severity -- `rules: { \"dead-candidates\": \"info\" }` \
-- which is the same gesture that changes any other rule's band, so there is no second vocabulary to \
learn. Their absence from the findings map means not analyzed, never analyzed-and-clean.";

const REPORTED_IN_CROSS_LAYER_FINDINGS: &str = "native analyses that are switched ON and still \
cannot appear in this `findings` map: they judge the CROSS-TREE JOIN and report into its own \
`crossLayerFindings` channel, which a per-tree output does not have. Their absence here is a \
property of the command that ran, not a verdict about your code — measured on a nine-repo corpus, \
running the cross-layer join over the very same nine single-tree configs turned nine such blanks \
into 975 findings, on 9 trees out of 9. To get a verdict from these, run the CROSS-LAYER JOIN over \
the same config; each surface names its own entry point for it.";

const RESIDUAL: &str = "a registered analysis in NONE of the three lists ran — EXCEPT for the two classes \
`registered` names, which key no `findings` entry under their own id by construction: an id that \
gates a score computation emits no finding at all, and an umbrella registration reports under the \
finer `schema/<label>` ids, so look for those rather than for the umbrella id. For every OTHER \
analysis in none of the three lists, absence from `findings` is a measured zero. That is all this object claims \
about it: like `zeroAdmissionRules` one field over, it says nothing about whether the analysis then \
had anything to judge — an analysis whose input channel is empty still ran.";

/// The RUN-FREE view of this legend, for the contract document `zzop_summary` serves once instead of
/// shipping these sentences on every reply.
///
/// Identical to [`native_analyses_meaning`] today, because this legend is already unconditional. It is
/// a separate entry point rather than a re-export so the document keeps reading a name that means "the
/// whole vocabulary" if the reply-side view ever grows a run-dependent key — which is exactly what
/// `packs_legend`'s pair already is, and the two must be looked up the same way or the next person
/// folds one of them wrong.
pub fn native_analyses_legend() -> BTreeMap<&'static str, &'static str> {
    native_analyses_meaning()
}

/// The legend for `nativeAnalyses`. `BTreeMap` like `packsLoadedMeaning`/`scoreMeanings`: key order
/// is a pure function of which keys exist, never of iteration luck.
///
/// Every key is present on every reply, including the two whose list may be empty. A legend that
/// appears only when its list is non-empty would make the empty case unexplained precisely where the
/// explanation matters — the reader who sees `disabled: []` is the one deciding whether a blank is a
/// verdict.
pub(super) fn native_analyses_meaning() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("registered", NATIVE_ANALYSES_REGISTERED_MEANING),
        ("disabled", NATIVE_ANALYSES_DISABLED_MEANING),
        ("shippedOff", NATIVE_ANALYSES_SHIPPED_OFF_MEANING),
        (
            "reportedInCrossLayerFindings",
            REPORTED_IN_CROSS_LAYER_FINDINGS,
        ),
        ("everythingElse", RESIDUAL),
    ])
}

#[cfg(test)]
mod tests {
    use super::{NATIVE_ANALYSES_REGISTERED_MEANING, RESIDUAL};

    /// Where `registered` stops describing the denominator and starts describing the two classes that
    /// key no `findings` entry under their own id.
    ///
    /// 🔴 This anchor used to carve the head of the sentence OUT of the count check, on the reasoning
    /// that a head count is "exhibited by the object itself". It was exhibited right up until it was
    /// not: the head said "the two lists below" and a THIRD list (`shippedOff`) shipped without the
    /// number following it (2026-09-12, review ledger V168). The carve-out was the whole defect — a
    /// guard written to stop hand counts held one exemption, and that is where the stale count sat.
    /// The head no longer counts either, so the check below reads the WHOLE string and this anchor is
    /// now only used to prove the class clauses are present (the floor), never to skip text.
    const CLASS_CLAUSE_ANCHOR: &str = "not the set of ids a finding can carry:";

    /// Every WHOLE word in `text` that states a small count, spelled or in digits, in order.
    ///
    /// Whole words, lowercased, because substring matching would read "one" out of "none" and "ten"
    /// out of "often" and report a hand count that is not there. An instrument that fails toward
    /// FALSE RED is still a broken instrument, and the caller canaries this one in both directions
    /// before trusting its empty answer.
    fn counted_words(text: &str) -> Vec<&str> {
        text.split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| {
                !w.is_empty()
                    && (w.chars().all(|c| c.is_ascii_digit())
                        || matches!(
                            w.to_ascii_lowercase().as_str(),
                            "one"
                                | "two"
                                | "three"
                                | "four"
                                | "five"
                                | "six"
                                | "seven"
                                | "eight"
                                | "nine"
                                | "ten"
                        ))
            })
            .collect()
    }

    /// 🔴 NO HAND COUNT in the class clauses. This sentence shipped "five of them ... and two are ..."
    /// while neither number had a source here — `zzop_metrics`' registrations are only countable at
    /// runtime and `zzop_rules_schema`'s umbrella subset is not countable from this crate at all — and
    /// the residual sentence one key over then made them load-bearing by deferring to this one. A
    /// sixth score-gating id would have made two sentences false at once, one of them by reference.
    ///
    /// A count is what would come BACK: the clauses read a little thinner without one, and the
    /// obvious "improvement" is to put the numbers in again. This test is the reason not to.
    ///
    /// FLOOR (the reason this cannot pass vacuously): a string that stopped containing the class
    /// descriptions would satisfy "no number word" while measuring nothing. Both class names are
    /// asserted present first, so the number check is only ever run over text that provably contains
    /// the clauses it is about.
    ///
    /// 🔴 SCOPE, widened 2026-09-12 (review ledger V168): the number check reads the WHOLE constant,
    /// head included. It used to read only the tail after [`CLASS_CLAUSE_ANCHOR`], and the head's one
    /// permitted count ("the two lists below") went false the day a third list shipped. A guard with
    /// an exemption is a guard that names where its next miss will be.
    #[test]
    fn registered_names_the_classes_that_key_no_finding_without_counting_them() {
        let (_, classes) = NATIVE_ANALYSES_REGISTERED_MEANING
            .split_once(CLASS_CLAUSE_ANCHOR)
            .expect(
                "`registered` must still separate what it counts from the classes that key no \
                 finding — if this anchor moved, re-aim the needle rather than deleting the test",
            );

        for class in ["gate score computations", "umbrella registrations"] {
            assert!(
                classes.contains(class),
                "FLOOR: the slice this test measures must contain the class clauses it is about, or \
                 `no number word` would be a statement about nothing — missing {class:?} in \
                 {classes:?}"
            );
        }

        // CANARY, both directions (the needle is a one-line scan, and a one-line scan that has never
        // been shown to fire is indistinguishable from one that cannot). The literal below is the
        // pre-repair clause verbatim; if the needle stops reading it as counted, the green above is
        // the needle's silence rather than the sentence's.
        assert_eq!(
            counted_words(
                " five of them gate score computations that emit no finding, and two are umbrella \
                 registrations whose findings carry finer `schema/<label>` ids."
            ),
            vec!["five", "two"],
            "the needle must see the hand count where one demonstrably was — and must not invent one \
             out of `none`/`no finding`/`schema/<label>`"
        );

        let counted = counted_words(NATIVE_ANALYSES_REGISTERED_MEANING);
        assert!(
            counted.is_empty(),
            "NOTHING in this sentence is COUNTED — not the classes that key no `findings` entry, and \
             not the lists it is the denominator for. This crate cannot derive either class \
             population (one is `zzop_metrics`' registrations, countable only at runtime; the other \
             is `zzop_rules_schema`'s umbrella subset, which this crate has no dependency edge to), \
             and the list count is owned by whoever adds the next list — which is exactly how `two` \
             survived `shippedOff` landing. Found {counted:?} in {NATIVE_ANALYSES_REGISTERED_MEANING:?}"
        );
    }

    /// 🔴 POSITION, not presence. The residual sentence's failure mode was that it stated its
    /// conclusion ("absence is a measured zero") with the exception nowhere near it, so a reader who
    /// stopped at the first clause took a claim that is false for two registration classes. Moving the
    /// carve-out behind the conclusion would keep every token and restore the defect.
    #[test]
    fn the_carve_out_precedes_the_measured_zero_claim() {
        let carve_out = RESIDUAL
            .find("EXCEPT for the two classes")
            .expect("the residual sentence must carve out the registrations that key no finding");
        let claim = RESIDUAL
            .find("absence from `findings` is a measured zero")
            .expect("the residual sentence must still make its claim");
        assert!(
            carve_out < claim,
            "the exception must reach the reader BEFORE the claim it limits ({carve_out} < {claim}): \
             {RESIDUAL}"
        );
    }

    /// The carve-out and the description it defers to stay on ONE owner. `registered` is where the two
    /// classes are described; this sentence may only POINT there while naming them well enough to be
    /// recognized. A rewrite that re-describes them here creates the second copy, and the two copies
    /// disagree the first time a registration class is added.
    #[test]
    fn the_carve_out_points_at_registered_rather_than_re_describing_it() {
        assert!(
            RESIDUAL.contains("`registered` names"),
            "the residual sentence must defer to `registered`'s own description: {RESIDUAL}"
        );
        for class in ["gates a score computation", "umbrella registration"] {
            assert!(
                RESIDUAL.contains(class),
                "a reader must be able to RECOGNIZE the excepted class without leaving this key — \
                 naming it is the point, counting or listing it is not: {RESIDUAL}"
            );
        }
    }
}
