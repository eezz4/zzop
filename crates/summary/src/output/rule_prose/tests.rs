//! Unit tests for the per-reply prose fold. Named `tests.rs` deliberately: this repo's surface
//! guards discriminate shipped source from test source by that name, and a test-only fixture file
//! under any other name gets counted as shipped.

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
    let table = fold(&mut shown).expect("both texts repeat and both are worth folding");
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
    let table = fold(&mut shown).unwrap();
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
    assert!(fold(&mut shown).is_none());
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
    let table = fold(&mut shown).expect("the long repeat pays for itself");
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
    assert!(fold(&mut shown).is_none(), "{shown:?}");
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
    let table = fold(&mut shown).expect("the second text pays");
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
    fold(&mut shown).unwrap();
    let m = shown[0]["message"]
        .as_str()
        .expect("message stays a string");
    assert!(!m.is_empty());
    assert!(m.contains("security/x"), "{m}");
    assert!(m.contains("ruleMessages"), "{m}");
    assert!(m.contains("messageRef"), "{m}");
}
