//! Unit tests for the per-reply prose fold. Named `tests.rs` deliberately: this repo's surface
//! guards discriminate shipped source from test source by that name, and a test-only fixture file
//! under any other name gets counted as shipped.
//!
//! The pins below drive [`fold_exact`] and [`template::fold`] directly rather than the composed
//! [`fold`], so a failure names which half broke. `super::tests` in the parent module owns the
//! composed, wire-shaped pins — the two are opposed on purpose and neither replaces the other.

use super::by_id::{BY_ID_MESSAGE, MESSAGE_BY, MESSAGE_BY_RULE_ID};
use super::*;

fn f(rule: &str, message: &str) -> serde_json::Value {
    serde_json::json!({ "ruleId": rule, "severity": "info", "message": message })
}

/// A prescription long enough that folding its repeats clears the byte gate with room to spare.
/// Every fixture below about KEY ASSIGNMENT goes through this: a text short enough to be left
/// inline would test key assignment by never reaching it, which is green for the wrong reason.
/// `tag` is what keeps two of these distinct where a pin needs them to be.
fn long(tag: &str) -> String {
    format!(
        "{tag}: {}",
        "one prescription sentence that this rule repeats verbatim on every finding it \
         produces, long enough to be worth storing once. "
            .repeat(20)
    )
}

/// The `#N` suffix exists for a rule that emits two DIFFERENT texts and repeats each — without
/// it the second text would overwrite the first under one key and every finding of that rule
/// would resolve to the wrong prose. Both directions are checked: the right text under the right
/// key, and each finding pointing at its own.
#[test]
fn a_rule_with_two_repeated_texts_gets_two_keys() {
    let (alpha, beta) = (long("alpha"), long("beta"));
    let mut shown = vec![f("r", &alpha), f("r", &beta), f("r", &alpha), f("r", &beta)];
    let table = fold_exact(&mut shown).expect("both texts repeat and both are worth folding");
    let table = table.as_object().unwrap();
    assert_eq!(table.len(), 2, "{table:?}");
    assert_eq!(
        table["r"], alpha,
        "first-appearance order takes the bare key"
    );
    assert_eq!(table["r#2"], beta, "{:?}", table.keys().collect::<Vec<_>>());
    assert_eq!(shown[0][MESSAGE_REF], "r");
    assert_eq!(shown[1][MESSAGE_REF], "r#2");
    assert_eq!(shown[2][MESSAGE_REF], "r");
    assert_eq!(shown[3][MESSAGE_REF], "r#2");
}

/// Two rules emitting the same sentence get one entry each, named after their own producer.
/// Sharing a key would make the table claim the other rule emitted it.
#[test]
fn two_rules_emitting_the_same_sentence_do_not_share_a_key() {
    let same = long("same");
    let mut shown = vec![f("a", &same), f("a", &same), f("b", &same), f("b", &same)];
    let table = fold_exact(&mut shown).unwrap();
    let table = table.as_object().unwrap();
    assert_eq!(table.len(), 2, "{table:?}");
    assert_eq!(table["a"], same);
    assert_eq!(table["b"], same);
    assert_eq!(shown[0][MESSAGE_REF], "a");
    assert_eq!(shown[2][MESSAGE_REF], "b");
}

/// Nothing repeated => no table, no `messageRef`, and not one message touched. The negative half
/// of every pin above: without it the fold could pay bytes on the trees it does not help.
///
/// The texts are LONG on purpose. Short ones would also produce no table, but for the other
/// reason — and a fixture that satisfies a pin two ways stops telling you which one held.
#[test]
fn unique_messages_are_left_exactly_as_they_were() {
    let (alpha, beta) = (long("alpha"), long("beta"));
    let mut shown = vec![f("r", &alpha), f("r", &beta)];
    assert!(fold_exact(&mut shown).is_none());
    assert_eq!(shown[0]["message"], alpha);
    assert_eq!(shown[1]["message"], beta);
    assert!(shown[0].get(MESSAGE_REF).is_none(), "{:?}", shown[0]);
    assert!(shown[1].get(MESSAGE_REF).is_none(), "{:?}", shown[1]);
}

/// THE GATE, both directions inside ONE reply — which is the whole reason the fold is partial.
/// An all-or-nothing gate has to pick a single answer for these two rules and either answer is
/// wrong for one of them: fold both and the short rule's two findings cost more than they
/// saved; fold neither and the long rule's copy ships twice for nothing.
#[test]
fn a_short_repeat_stays_inline_while_a_long_one_beside_it_folds() {
    let prose = long("worth it");
    let short = "Clear this interval when the component unmounts.";
    let mut shown = vec![
        f("long/rule", &prose),
        f("short/rule", short),
        f("long/rule", &prose),
        f("short/rule", short),
    ];
    let table = fold_exact(&mut shown).expect("the long repeat pays for itself");
    let table = table.as_object().unwrap();
    assert_eq!(
        table.len(),
        1,
        "only the text whose fold saves bytes belongs here: {:?}",
        table.keys().collect::<Vec<_>>()
    );
    assert_eq!(table["long/rule"], prose);
    assert_eq!(shown[0][MESSAGE_REF], "long/rule");
    assert_eq!(
        shown[1]["message"], short,
        "the short repeat keeps its own text: {:?}",
        shown[1]
    );
    assert!(shown[1].get(MESSAGE_REF).is_none(), "{:?}", shown[1]);
}

/// A reply whose ONLY repeat is short folds nothing at all — absent table, absent legend, every
/// message untouched. This is the case the old `n > 1` gate got backwards, and it is not exotic:
/// 45 of the 118 shipped rule messages sit below the n=2 break-even.
#[test]
fn a_reply_whose_only_repeat_is_short_folds_nothing() {
    let short = "Clear this interval when the component unmounts.";
    let mut shown = vec![f("r", short), f("r", short), f("r", short)];
    assert!(fold_exact(&mut shown).is_none(), "{shown:?}");
    for finding in &shown {
        assert_eq!(finding["message"], short, "{finding:?}");
        assert!(finding.get(MESSAGE_REF).is_none(), "{finding:?}");
    }
}

/// Keys are dense over what actually folded, not over what was merely repeated: a rule whose
/// FIRST distinct text was rejected by the gate gets the bare key for its second, never `r#2`
/// with no `r` beside it. The table would otherwise advertise a sibling entry that is not there.
#[test]
fn a_rejected_candidate_does_not_reserve_a_key() {
    let short = "Clear this interval when the component unmounts.";
    let prose = long("second");
    let mut shown = vec![f("r", short), f("r", &prose), f("r", short), f("r", &prose)];
    let table = fold_exact(&mut shown).expect("the second text pays");
    let table = table.as_object().unwrap();
    assert_eq!(table.len(), 1, "{:?}", table.keys().collect::<Vec<_>>());
    assert_eq!(table["r"], prose, "{:?}", table.keys().collect::<Vec<_>>());
    assert_eq!(shown[1][MESSAGE_REF], "r");
    assert!(shown[0].get(MESSAGE_REF).is_none(), "{:?}", shown[0]);
}

/// A folded finding keeps a NON-EMPTY string `message` that names its own key, so a reader who
/// only ever looks at `message` is told where the text went instead of finding a blank. The
/// frozen half of the wire contract is the key and its type; this pin holds both.
#[test]
fn a_folded_finding_keeps_a_non_empty_message_naming_its_key() {
    let prose = long("secret");
    let mut shown = vec![
        f("security/x", &prose),
        f("security/x", &prose),
        f("security/x", &prose),
    ];
    fold_exact(&mut shown).unwrap();
    let m = shown[0]["message"]
        .as_str()
        .expect("message stays a string");
    assert!(!m.is_empty());
    assert!(m.contains("security/x"), "{m}");
    assert!(m.contains("ruleMessages"), "{m}");
    assert!(m.contains("messageRef"), "{m}");
}

// ---------------------------------------------------------------------------
// THE TEMPLATE HALF. Driven through [`template::fold`] directly so a failure says which half broke.
// ---------------------------------------------------------------------------

/// Reads one finding's residues back out of the wire shape, so a pin can rebuild through the same
/// public interleave a consumer uses instead of through a producer-side helper.
fn parts_of(finding: &serde_json::Value) -> Vec<String> {
    finding[TEMPLATE_PARTS]
        .as_array()
        .unwrap_or_else(|| panic!("a templated finding carries a `templateParts` array: {finding}"))
        .iter()
        .map(|v| v.as_str().expect("a part is a string").to_string())
        .collect()
}

fn segments_of(table: &serde_json::Value, rule: &str) -> Vec<String> {
    table[rule]
        .as_array()
        .unwrap_or_else(|| panic!("no template for {rule}: {table}"))
        .iter()
        .map(|v| v.as_str().expect("a segment is a string").to_string())
        .collect()
}

/// A message that VARIES at both ends gets an empty segment at each, which is the whole reason the
/// model is "segment, part, segment, ... segment" rather than "part between two anchors". Without
/// the empty edges a rule whose subject leads its sentence could not template at all — and that is
/// the majority shape (`Model X ...`, `` `symbol` is exported and ... ``).
#[test]
fn a_template_carries_empty_edge_segments_when_the_message_varies_at_its_ends() {
    let body = long("edges");
    let mk = |lead: &str, tail: &str| format!("{lead} {body} {tail}");
    let originals = [mk("Xalpha", "P1."), mk("Ybeta", "P2."), mk("Zgamma", "P3.")];
    let mut shown: Vec<_> = originals.iter().map(|m| f("r", m)).collect();
    let table =
        template::fold(&mut shown).expect("one long shared body across three findings pays");
    let segs = segments_of(&table, "r");
    assert_eq!(
        segs.first().map(String::as_str),
        Some(""),
        "a message beginning with per-finding text needs an empty leading segment: {segs:?}"
    );
    assert_eq!(
        segs.last().map(String::as_str),
        Some(""),
        "...and the same at the other end: {segs:?}"
    );
    for (i, original) in originals.iter().enumerate() {
        assert_eq!(
            &template::reconstruct(&segs, &parts_of(&shown[i])),
            original,
            "rebuild differed from the original: {:?}",
            shown[i]
        );
    }
}

/// A segment that occurs MORE THAN ONCE in the message. The split takes each segment at its earliest
/// possible position, so a fixture where the same literal appears twice is the one that would expose
/// a splitter that grabbed the wrong occurrence — and it would do so as a WRONG REBUILD, not as an
/// error, which is why the byte comparison rather than "did it return Some" is the assertion.
#[test]
fn a_segment_that_occurs_twice_still_rebuilds_byte_identically() {
    let body = long("dup");
    let mk = |v: &str| format!("Model {v} {body} Model {v} again.");
    let originals = [mk("Xuser"), mk("Ybooking"), mk("Zwebhook")];
    let mut shown: Vec<_> = originals.iter().map(|m| f("r", m)).collect();
    let table = template::fold(&mut shown).expect("the shared body pays");
    let segs = segments_of(&table, "r");
    for (i, original) in originals.iter().enumerate() {
        assert_eq!(
            &template::reconstruct(&segs, &parts_of(&shown[i])),
            original,
            "rebuild differed from the original: {:?}",
            shown[i]
        );
    }
    // The varying value really is on the finding rather than baked into a segment, in BOTH
    // directions — otherwise this pin passes on a fold that stored three whole messages.
    assert!(parts_of(&shown[0]).iter().any(|p| p.contains("Xuser")));
    assert!(!segs.iter().any(|s| s.contains("Xuser")), "{segs:?}");
}

/// A finding the EXACT fold already addressed is never templated on top. Two addresses for one text
/// is not a size question — it is a reply that says two different things about where its own prose
/// lives, and a consumer reading whichever field it checks first would be reading a coin flip.
#[test]
fn a_finding_already_pointing_at_the_message_table_is_never_templated_too() {
    let prose = long("same");
    let mut shown = vec![f("r", &prose), f("r", &prose), f("r", &prose)];
    fold_exact(&mut shown).expect("three identical long texts fold");
    assert!(
        template::fold(&mut shown).is_none(),
        "nothing is left inline to template: {shown:?}"
    );
    for finding in &shown {
        assert!(finding.get(MESSAGE_REF).is_some(), "{finding:?}");
        assert!(finding.get(TEMPLATE_PARTS).is_none(), "{finding:?}");
    }
}

/// Alignment is quadratic in tokens, so a message past the cap folds NOTHING rather than spending a
/// reply's whole budget on one finding that carried a data structure as prose. The pin exists
/// because the failure mode of a missing cap is a hang, which no assertion about output would catch.
#[test]
fn a_message_past_the_token_cap_is_left_inline() {
    let huge = "word ".repeat(4000);
    let originals = [format!("{huge} A"), format!("{huge} B")];
    let mut shown: Vec<_> = originals.iter().map(|m| f("r", m)).collect();
    assert!(
        template::fold(&mut shown).is_none(),
        "a message past the token cap must not be mined"
    );
    for (i, original) in originals.iter().enumerate() {
        assert_eq!(shown[i]["message"], *original, "{:?}", shown[i]);
        assert!(shown[i].get(TEMPLATE_PARTS).is_none(), "{:?}", shown[i]);
    }
}

// ---------------------------------------------------------------------------------------------
// THE BY-ID LANE. Produced here (`point_at_rule_id`), unlike the two folds' subjects, so these pins
// drive the real function rather than a hand-built fixture wherever they can.
// ---------------------------------------------------------------------------------------------

/// A real bundled rule id and the exact text the facade hands over for it, so the pin exercises the
/// same equality the shipped path does instead of asserting against a string this file invented.
fn bundled_probe() -> (&'static str, &'static str) {
    let id = "db/update-delete-no-where";
    let text = zzop_facade::bundled_verbatim_message(id)
        .expect("a bundled rule id must resolve, or this pin is testing nothing");
    (id, text)
}

/// The whole contract of the lane in one pin: prose a reader can rebuild leaves, the FIELD says so,
/// and the sentence is not the signal.
#[test]
fn a_rebuildable_message_is_replaced_by_a_pointer_and_a_field() {
    let (id, text) = bundled_probe();
    let mut shown = vec![f(id, text)];
    assert!(point_at_rule_id(&mut shown), "the probe must fire");
    assert_eq!(shown[0]["message"], BY_ID_MESSAGE);
    assert_eq!(
        shown[0][MESSAGE_BY], MESSAGE_BY_RULE_ID,
        "the discriminator is a FIELD — a consumer must never have to parse the sentence: {:?}",
        shown[0]
    );
}

/// The round-18 IRREVERSIBLE, pinned where it can still fire. The lane originally had NO field, so
/// the only signal was the sentence — which `VERSIONING.md` frees to change. The repair was the
/// field, and what can rot now is the pair drifting apart: the legend telling readers to look at one
/// spelling while the pass writes another. Both come from this file, so nothing else would notice.
///
/// There is deliberately no `is_by_id` helper to test. This module emits the field and never reads it
/// back, so it cannot regress into detecting its own marker by wording — a predicate whose only
/// callers were tests would have been a detector this repo does not ship.
#[test]
fn the_legend_names_the_field_and_value_the_pass_actually_writes() {
    let (id, text) = bundled_probe();
    let mut shown = vec![f(id, text)];
    assert!(point_at_rule_id(&mut shown));
    let field = shown[0]
        .as_object()
        .expect("a finding is an object")
        .keys()
        .find(|k| k.as_str() == MESSAGE_BY)
        .expect("the pass wrote the field");
    assert!(
        MESSAGE_BY_ID_MEANING.contains(field.as_str()),
        "the legend must name the field a reader is told to look for; it says neither {MESSAGE_BY:?} \
         nor anything like it"
    );
    assert!(
        MESSAGE_BY_ID_MEANING.contains(MESSAGE_BY_RULE_ID),
        "and the value that field carries"
    );
    assert!(
        BY_ID_MESSAGE.contains("messageByIdMeaning"),
        "and the pointer must say where its own explanation is: {BY_ID_MESSAGE:?}"
    );
}
/// A rule this binary does not carry keeps every byte. Round 18 measured the failure this prevents: a
/// pack in `zzop/rules/` produced six findings, all six pointered, and both resolvers the pointer names
/// refused them — `zzop explain` with exit 1 and the MCP resource with -32602.
#[test]
fn a_rule_the_resolvers_cannot_answer_for_is_never_pointed_away() {
    assert!(
        zzop_facade::bundled_verbatim_message("user-pack/not-compiled-in").is_none(),
        "sanity: the probe id must really be outside the bundled corpus"
    );
    let mut shown = vec![f("user-pack/not-compiled-in", &long("user pack prose"))];
    let before = shown[0]["message"].clone();
    assert!(!point_at_rule_id(&mut shown), "nothing should fire");
    assert_eq!(shown[0]["message"], before);
    assert!(shown[0].get(MESSAGE_BY).is_none(), "{:?}", shown[0]);
}

/// A native analysis has no declared message to rebuild from, and the lookup is what skips it.
#[test]
fn a_native_finding_is_never_pointed_away() {
    let mut shown = vec![f("circular", &long("native prose"))];
    assert!(!point_at_rule_id(&mut shown));
    assert!(shown[0].get(MESSAGE_BY).is_none());
}

/// A message that DIFFERS from what the rule alone would have produced is a near-miss rewrite: it
/// quotes a token read out of the reader's own source, which no rule id can rebuild.
#[test]
fn a_message_the_rule_alone_would_not_have_produced_keeps_every_byte() {
    let (id, text) = bundled_probe();
    let rewritten = format!("{text} Note: a comment on this line reads `zzop-something-ok`.");
    let mut shown = vec![f(id, &rewritten)];
    assert!(
        !point_at_rule_id(&mut shown),
        "a near-miss must never be pointed away"
    );
    assert_eq!(shown[0]["message"], rewritten);
}

/// A reply carrying the pointer must carry the sentence that explains it, and one that does not must
/// be byte-identical to a reply from before this lane existed.
#[test]
fn the_by_id_legend_rides_exactly_when_a_pointer_does() {
    let (id, text) = bundled_probe();
    let mut with = vec![f(id, text)];
    let mut out = serde_json::json!({});
    fold(&mut with).publish(&mut out);
    assert_eq!(out["messageByIdMeaning"], MESSAGE_BY_ID_MEANING, "{out}");

    let mut without = vec![f("r", "inline prose"), f("q", "other inline prose")];
    let mut out = serde_json::json!({});
    fold(&mut without).publish(&mut out);
    assert!(
        out.get("messageByIdMeaning").is_none(),
        "additive-only: a reply with no pointer must be byte-identical to one from before this \
         existed. Got: {out}"
    );
}

/// No finding may hold two addresses for one text. The by-id pass runs first, so the folds must see
/// the pointers it wrote and leave them alone.
#[test]
fn a_pointed_finding_gains_no_second_address() {
    let (id, text) = bundled_probe();
    let mut shown = vec![f(id, text), f(id, text), f(id, text)];
    let folded = fold(&mut shown);
    let mut out = serde_json::json!({});
    folded.publish(&mut out);
    assert!(out.get("ruleMessages").is_none(), "{out}");
    assert!(out.get("ruleMessageTemplates").is_none(), "{out}");
    for finding in &shown {
        assert_eq!(finding[MESSAGE_BY], MESSAGE_BY_RULE_ID);
        assert!(finding.get(MESSAGE_REF).is_none(), "{finding}");
        assert!(finding.get(TEMPLATE_PARTS).is_none(), "{finding}");
    }
}

/// The legend's claim about the OTHER two lanes has to stay true of them. All three sentences are in
/// this file; a reword that made `ruleMessages` lossy, or this one lossless, would leave the pair
/// asserting opposite things about one reply with nothing red.
#[test]
fn the_two_legends_do_not_promise_the_same_thing() {
    assert!(
        RULE_MESSAGES_MEANING.contains("without a second request"),
        "the fold's promise is that everything is reachable from this reply alone"
    );
    assert!(
        MESSAGE_BY_ID_MEANING.contains("second request"),
        "and the by-id legend's whole job is to say that this one is NOT"
    );
}
