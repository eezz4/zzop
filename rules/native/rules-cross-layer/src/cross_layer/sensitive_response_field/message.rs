//! The finding text for `cross-layer/sensitive-response-field` — the LANDING, and the one `format!`
//! that splices it ahead of the imperative.
//!
//! ## Why this is its own module
//! Not because the sentences are shared (they have exactly one caller — unlike `rules-schema`'s
//! `message::landing`, whose constants are spliced into several rules and are in a module so that the
//! ONE spelling of each stays one). Here the reason is mechanical: the parent file stood at 299 lines
//! against `scripts/check-max-file-lines.sh`'s 300-line limit, and that guard REFUSES to grandfather a
//! new entry into its baseline. A landing had to land somewhere, and the split is the answer the guard
//! is asking for.
//!
//! ## What the landing is for (rule-quality.md §27, third leg)
//! The parent rule already carried a strong DISQUALIFIER — "the value may be benign (a public id) and
//! an auth route returning a token can be by design" — ahead of a CONDITIONED imperative ("Verify the
//! field belongs in the wire contract; if not, remove it"). Both §27 forms were already satisfied for
//! the question *is this finding true*. What was absent is the other question: what a reader who
//! answers "no, it does not belong" pays for the edit. Two things, and neither was stated:
//!
//! 1. **The finding is route-scoped; both remedies are class-scoped.** Dropping the property or
//!    marking it `@Exclude()` acts on the DTO CLASS, so it lands on every OTHER route whose declared
//!    return type is that class — including one where handing the value out is the entire point.
//! 2. **Nothing fails to build.** An HTTP consume site is not type-checked against the provider's DTO,
//!    so a consumer that reads the removed field keeps compiling and gets `undefined` at runtime.
//!
//! (2) is sharper in the CRITICAL arm than anywhere else in this crate, and that is why the arm gets
//! its own sentence: the witnessed consumer count is simultaneously the evidence that escalated the
//! finding and the population the prescribed edit breaks. The message named it only as the former.
//!
//! ## What is deliberately NOT claimed here
//! That a shared DTO is COMMON. It is not measured — the only live corpus finding
//! (cal.com `POST /v2/auth/oauth2/token`) is a single route, and the fixture tree's three findings are
//! three distinct shapes. The landing states a MECHANICAL property of `class-transformer` and of
//! deleting a class property, never a frequency, for the reason §32 and §30 both record: a message
//! that teaches a reader to doubt true findings costs more than the one it saves.

/// The LANDING, spliced AHEAD of the imperative and BEHIND the disqualifier (§27 order). Pinned by
/// POSITION rather than existence — see `tests::the_removal_landing_precedes_the_imperative`, whose
/// invalidation probe moves this behind `Verify the field belongs` and leaves every token present.
const REMOVAL_LANDING: &str = "WHAT REMOVAL COSTS IS NOT SCOPED THE WAY THIS FINDING IS: this \
     finding names ONE route, but both remedies act on the DTO CLASS — dropping the property, or \
     marking it `@Exclude()`, applies wherever that class is serialized, so every OTHER route \
     declaring the same return type loses the field in the same edit. That includes the route where \
     handing the value out is the whole point: a create-or-rotate response that shows a plaintext key \
     exactly once, or a token endpoint whose body IS the credential. List the routes declaring this \
     shape before you edit it. And nothing will tell you afterwards — an HTTP caller is not \
     type-checked against the provider's DTO, so a consumer that reads this field keeps compiling and \
     starts receiving `undefined` at runtime.";

/// The critical arm's extra sentence. Only this arm has a witnessed consumer set to name, and naming
/// it is the point: the count that ESCALATED the finding is the count the edit breaks.
fn witnessed_consumer_tail(consumer_count: u32) -> String {
    // Agreement follows the count, the same way the caller's own `site_word` does. The singular is
    // not an edge case to skip: escalation needs ONE edge, and the fixture's own critical finding
    // (`xbe/session.controller.ts:15`) has exactly one consumer — a hardcoded plural shipped
    // "The 1 witnessed call site ... ARE" there on the first draft.
    let (site_word, verb) = if consumer_count == 1 {
        ("call site", "IS")
    } else {
        ("call sites", "ARE")
    };
    format!(
        " The {consumer_count} witnessed {site_word} counted above {verb} that population — the same \
         evidence that raised this finding to critical is the set the edit silently breaks."
    )
}

/// Builds the finding message. `exposure` is the arm clause the caller already selected alongside the
/// severity; `consumer_count` is 0 on the warning arm, where the exposure clause says so itself.
///
/// Everything except `{REMOVAL_LANDING}{tail}` is byte-identical to the sentence this rule shipped
/// before 2026-08-30 — the landing is an INSERTION between the disqualifier and the imperative, not a
/// rewrite, so the `contains` pins that predate it keep testing what they were written to test.
///
/// `disable_tail` is passed IN rather than built here, and that is not a style choice: this rule's id
/// belongs beside the `Finding` the parent constructs, and `rule_contracts`' native-message contract
/// reads `disable_hint(` and `rule_id: "` as a FILE-level co-occurrence. Building the hint here would
/// leave the parent holding a `rule_id` literal with no exclude-hint evidence in its own bytes — the
/// exact false positive that contract's own module doc predicts for a message split around
/// `disable_hint`'s output. Keeping both in the parent satisfies the guard by making its claim true
/// rather than by adding a doc-comment mention for it to find.
pub(super) fn finding_message(
    key: &str,
    field_word: &str,
    fields_list: &str,
    exposure: &str,
    consumer_count: u32,
    disable_tail: &str,
) -> String {
    let tail = if consumer_count > 0 {
        witnessed_consumer_tail(consumer_count)
    } else {
        String::new()
    };
    format!(
        "route `{key}` declares a response shape containing sensitive-named {field_word} \
         `{fields_list}`, {exposure}. Evidence is the declared field NAME only — the value may \
         be benign (a public id) and an auth route returning a token can be by design — and the \
         DECLARATION only: runtime serialization (`@Exclude` decorators, `toJSON` methods, interceptors) is not \
         read. {REMOVAL_LANDING}{tail} Verify the field belongs in the wire contract; if not, remove it from the \
         response DTO or strip it before serialization. Distinct from the `security` pack's \
         secret rules, which match literal secret VALUES in source — this reads declared \
         response field names, so both can legitimately fire on one file. {disable_tail}",
    )
}
