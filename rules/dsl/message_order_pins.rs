//! POSITION pins for rule messages — the ONE home for "this clause is reached before that
//! instruction" across every DSL pack (`1.architecture/rules/rule-quality.md` §27, §33, §37).
//!
//! Not a pack directory: this is a plain module file that each `rules/dsl/<pack>/<pack>.rs` pulls in
//! with `#[path = "../message_order_pins.rs"] mod message_order_pins;`. Every pack root is its own
//! `[[test]]` target — a separate crate root — so there is no ordinary module they can share, and
//! `rules/src/lib.rs` is deliberately not a library. `#[path]` is what is left, and it costs no
//! manifest entry, no dependency edge and no crate boundary.
//!
//! WHY POSITION AND NOT PRESENCE. A reader who acts on the FIRST instruction never reaches a caveat
//! placed behind it, so asserting a caveat is merely PRESENT proves nothing: the invalidation probe
//! is to move the clause after the imperative with every token still spelled in the message, at
//! which point a `contains` pin stays green and these go red. That is how the ordering defect shipped
//! green across six db rules, five security rules and four reliability rules at once.
//!
//! WHY FOUR HELPERS AND NOT ONE. They make DIFFERENT claims, and the names say which:
//!
//! | helper | what the left offset IS | what the clause body gets |
//! |---|---|---|
//! | [`assert_disqualifier_summary_precedes_imperative`] | a SUMMARY token planted ahead of the verb | presence only |
//! | [`assert_disqualifier_clause_precedes_imperative`]  | the disqualifying clause ITSELF          | exactly-once + position |
//! | [`assert_clauses_precede_imperative`]               | each of N clause fragments               | exactly-once + position |
//! | [`assert_landing_precedes_imperative`]              | a shared LANDING constant                | exactly-once + position |
//!
//! The first two are not interchangeable and merging them would make one of them lie. §27's third
//! form conditions the imperative itself (`IF <premise>: <imperative>`); there the thing in front of
//! the verb is a summary of a longer clause that stays behind it, and the summary IS the working
//! part. A pure MOVE plants no summary, so for those the left offset is the clause, a strictly
//! stronger claim — and the summary form's panic prose ("the condition in front of the remedy is
//! that clause's SUMMARY") would be false if it were used there.
//!
//! The landing family is a DIFFERENT DECISION from the disqualifier family, not a variant of it:
//! §33/§37 landings say what the reader's own correct edit costs, not when the finding is wrong.
//! They share this file because they share the mechanism, and nothing else.
//!
//! `rule` is the bare rule id, which `rule_contracts::id_hygiene` already keeps unique across every
//! pack, so no pack prefix is needed to read a failure.
//!
//! THE EXACTLY-ONCE ASSERTIONS ARE LOAD-BEARING, not ceremony: an index comparison against a needle
//! that occurs twice compares whichever copy `find` reaches first, which is not the claim being made.

#![allow(
    dead_code,
    reason = "One shared module, eight pack roots, four helpers — no pack uses all four, and every \
              `#[path]`-included module is compiled separately into each test binary that includes \
              it. Splitting the file per helper to silence this would put the summary form and the \
              clause form back in different files, which is the state this file exists to end."
)]

/// Asserts that a SUMMARY of the disqualifying clause is reached before the imperative, and that the
/// clause body is still somewhere in the message.
///
/// §27's third form: the imperative itself is conditioned (`IF <premise>: <imperative>`), so what
/// sits in front of the verb is a headline for a longer clause that stays behind it. The body is
/// asserted PRESENT and nothing more — with the body gone the reader has a headline with nothing
/// under it to check the premise against, but where the body sits is not this pin's claim.
///
/// Use [`assert_disqualifier_clause_precedes_imperative`] instead whenever the repair was a pure
/// move: there is no planted summary there, and this function's panic text would lie.
pub(crate) fn assert_disqualifier_summary_precedes_imperative(
    rule: &str,
    message: &str,
    condition: &str,
    imperative: &str,
    disqualifier: &str,
) {
    assert!(
        message.contains(disqualifier),
        "{rule}: the clause that disqualifies this rule's own remedy left the message entirely — \
         missing {disqualifier:?}. The condition in front of the remedy is that clause's SUMMARY; \
         with the clause gone the reader cannot check the premise the remedy now hangs on. \
         In: {message}"
    );
    for needle in [condition, imperative] {
        assert_eq!(
            message.matches(needle).count(),
            1,
            "{rule}: {needle:?} must be spelled exactly ONCE, or the offset comparison below \
             compares against an arbitrary copy. In: {message}"
        );
    }
    let at_condition = message.find(condition).expect("asserted above");
    let at_imperative = message.find(imperative).expect("asserted above");
    assert!(
        at_condition < at_imperative,
        "{rule}: the imperative sits at byte {at_imperative}, AHEAD of the condition that \
         invalidates it at byte {at_condition} — a reader who edits on the first instruction never \
         reaches the premise. Move the condition or condition the verb; do not rewrite either. \
         In: {message}"
    );
}

/// Asserts that the disqualifying clause ITSELF is reached before the imperative.
///
/// The strict form, for repairs that are pure moves: no summary token was planted in front of the
/// remedy, so the left-hand offset is the clause rather than a headline for it.
pub(crate) fn assert_disqualifier_clause_precedes_imperative(
    rule: &str,
    message: &str,
    disqualifier: &str,
    imperative: &str,
) {
    let n_disqualifier = message.matches(disqualifier).count();
    assert_eq!(
        n_disqualifier, 1,
        "{rule}: the clause that disqualifies this rule's own finding must be spelled exactly ONCE \
         — {disqualifier:?} occurs {n_disqualifier} time(s). At zero it left the message entirely \
         and the reader can no longer tell when this finding is wrong; above one the offset \
         comparison below compares against an arbitrary copy. In: {message}"
    );
    assert_eq!(
        message.matches(imperative).count(),
        1,
        "{rule}: the imperative {imperative:?} must be spelled exactly ONCE, or the offset \
         comparison below compares against an arbitrary copy. In: {message}"
    );
    let at_disqualifier = message.find(disqualifier).expect("asserted above");
    let at_imperative = message.find(imperative).expect("asserted above");
    assert!(
        at_disqualifier < at_imperative,
        "{rule}: the imperative sits at byte {at_imperative}, AHEAD of the clause that disqualifies \
         this finding at byte {at_disqualifier} — a reader who edits on the first instruction never \
         reaches it. Move the clause or condition the verb; do not rewrite either. In: {message}"
    );
}

/// Asserts that EVERY fragment of a multi-part counter-indication is reached before the imperative.
///
/// The same claim as [`assert_disqualifier_clause_precedes_imperative`], made once per fragment,
/// for the rules whose counter-indication is a block rather than a sentence. Each fragment carries
/// one leg — the precondition, the shape that violates it, how the edit breaks — so a merely topical
/// token would let a future edit keep the word and drop the warning.
pub(crate) fn assert_clauses_precede_imperative(
    rule: &str,
    message: &str,
    imperative: &str,
    clauses: &[&str],
) {
    assert_eq!(
        message.matches(imperative).count(),
        1,
        "{rule}: the imperative {imperative:?} must be spelled exactly ONCE, or the offset \
         comparisons below compare against an arbitrary copy. In: {message}"
    );
    let at_imperative = message.find(imperative).expect("asserted above");
    for clause in clauses {
        let n = message.matches(clause).count();
        assert_eq!(
            n, 1,
            "{rule}: the counter-indication must name {clause:?} exactly ONCE — it occurs {n} \
             time(s). At zero the warning left the message; above one the offset comparison below \
             compares against an arbitrary copy. In: {message}"
        );
        let at = message.find(clause).expect("asserted above");
        assert!(
            at < at_imperative,
            "{rule}: the counter-indication ({clause:?} at {at}) must PRECEDE the imperative remedy \
             (at {at_imperative}) — a reader who acts on the first instruction never reaches a \
             caveat that comes after it. In: {message}"
        );
    }
}

/// Asserts that a shared LANDING clause is reached before the imperative it guards.
///
/// A different decision from the three above (§33/§37, not §27-a): a landing says what the reader's
/// own CORRECT edit costs — what it strands, locks or takes down — rather than when the finding is
/// wrong. The landing is passed in because each family keys one byte-identical constant, and a
/// position pin needs ONE spelling to index.
pub(crate) fn assert_landing_precedes_imperative(
    rule: &str,
    message: &str,
    landing: &str,
    imperative: &str,
) {
    for needle in [landing, imperative] {
        assert_eq!(
            message.matches(needle).count(),
            1,
            "{rule}: {needle:?} must be spelled exactly ONCE, or the offset comparison below \
             compares against an arbitrary copy. In: {message}"
        );
    }
    let at_landing = message.find(landing).expect("asserted above");
    let at_imperative = message.find(imperative).expect("asserted above");
    assert!(
        at_landing < at_imperative,
        "{rule}: the imperative sits at byte {at_imperative}, AHEAD of the landing that says what \
         following it costs, at byte {at_landing} — a reader who acts on the first instruction pays \
         that cost and reads the sequencing afterwards. Move the landing; do not rewrite it. \
         In: {message}"
    );
}
