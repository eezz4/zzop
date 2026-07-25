//! Near-miss disclosure: a comment SHAPED like a zzop suppress marker that this rule does not honor is
//! named in the emitted finding's message (`markers::message_with_near_miss`).
//!
//! The measured defect: an author writes `// as-ok:` next to an `as-cast` finding whose honored marker is
//! `as-cast-ok`. The comment suppresses nothing and zzop said nothing — the worst direction of silence.
//! These tests pin both sides: the disclosure fires and names BOTH tokens, the finding still fires, and
//! ordinary prose ending in `-ok` is never accused of being a typo'd marker.

use super::super::test_support::{method, rule_pack, scan_pack};
use super::super::RulePackDef;
use super::marker_method_pack;

/// The rule's own message, unchanged — the baseline every "message unchanged" assertion below compares to.
const BASE_MESSAGE: &str = "m";

/// The measured defect reproduced: rule id `as-cast` (honored marker `as-cast-ok`) against an author who
/// wrote the shorter `// as-ok`. The `line_pattern` deliberately targets a CALL (`widen(`) that no comment
/// in these fixtures contains, so the comment lines themselves never produce a second finding and each
/// assertion below is about exactly one finding's message.
fn near_miss_pack(file_pattern: &str) -> RulePackDef {
    rule_pack(&format!(
        r#"{{"id":"as-cast","severity":"info","message":"m","matcher":{{"type":"line-scan","file_pattern":"{file_pattern}","line_pattern":"\\bwiden\\("}}}}"#
    ))
}

fn near_miss_line_pack() -> RulePackDef {
    near_miss_pack("\\\\.ts$")
}

#[test]
fn wrong_shaped_marker_on_the_anchor_line_is_named_in_the_message() {
    let f = scan_pack(
        &near_miss_line_pack(),
        "f.ts",
        "const x = widen(y); // as-ok: guaranteed by caller\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "disclosure must not change which findings fire");
    assert_eq!(
        f[0].message,
        "m Note: a comment on this line (or the line directly above it) reads `as-ok`, which does not \
         suppress this rule — the marker this rule honors is `as-cast-ok`, so this finding still fires.",
        "the message must name BOTH the token found and the marker actually honored"
    );
}

#[test]
fn wrong_shaped_marker_on_the_line_above_is_named_in_the_message() {
    let f = scan_pack(
        &near_miss_line_pack(),
        "f.ts",
        "// as-ok\nconst x = widen(y);\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(
        f[0].message.contains("reads `as-ok`") && f[0].message.contains("honors is `as-cast-ok`"),
        "a bare marker-shaped comment (no `:`) on the lookback line is still an attempt: {}",
        f[0].message
    );
}

#[test]
fn the_honored_marker_still_suppresses_and_is_never_disclosed_as_a_near_miss() {
    let f = scan_pack(
        &near_miss_line_pack(),
        "f.ts",
        "const x = widen(y); // as-cast-ok: vetted\n",
        vec![],
    );
    assert!(
        f.is_empty(),
        "no regression: the right marker still suppresses {f:?}"
    );
}

#[test]
fn prose_word_ending_in_ok_inside_a_sentence_is_not_accused() {
    // `half-ok` is marker-SHAPED but is followed by more prose, not by `:` or the end of the comment —
    // the accepted shape is "first token of the comment, terminated by `:` or end-of-comment", exactly
    // how a real marker is written. A sentence cannot reach it.
    let f = scan_pack(
        &near_miss_line_pack(),
        "f.ts",
        "const x = widen(y); // half-ok for now, revisit before release\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].message, BASE_MESSAGE, "prose must not be accused");
}

#[test]
fn a_marker_shaped_token_not_adjacent_to_the_comment_leader_is_not_accused() {
    // `TODO: not-ok` — the `-ok` word is not the comment's first token, so it is prose, not an attempt.
    let f = scan_pack(
        &near_miss_line_pack(),
        "f.ts",
        "const x = widen(y); // TODO: not-ok yet\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].message, BASE_MESSAGE);
}

#[test]
fn no_marker_at_all_leaves_the_message_byte_identical() {
    let f = scan_pack(
        &near_miss_line_pack(),
        "f.ts",
        "const x = widen(y);\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].message, BASE_MESSAGE);
}

#[test]
fn near_miss_disclosure_is_deterministic_across_repeated_scans() {
    let src = "// as-ok: one\nconst x = widen(y); // other-ok\n";
    let a = scan_pack(&near_miss_line_pack(), "f.ts", src, vec![]);
    let b = scan_pack(&near_miss_line_pack(), "f.ts", src, vec![]);
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(a[0].message, b[0].message);
    assert!(
        a[0].message.contains("reads `as-ok`"),
        "earliest window line, leftmost token wins: {}",
        a[0].message
    );
}

#[test]
fn a_dash_dash_near_miss_is_disclosed_only_inside_a_sql_file() {
    // Leader parity with suppression: `--` is a comment in `.sql` only, so the same text discloses there
    // and stays silent in `.ts` (where `--x` is a decrement, not a comment).
    let sql = scan_pack(
        &near_miss_pack("\\\\.sql$"),
        "f.sql",
        "SELECT widen(x); -- as-ok: vetted\n",
        vec![],
    );
    assert_eq!(sql.len(), 1, "{sql:?}");
    assert!(
        sql[0].message.contains("reads `as-ok`"),
        "{}",
        sql[0].message
    );

    let f = scan_pack(
        &near_miss_pack("\\\\.(ts|sql)$"),
        "f.ts",
        "const x = widen(y); -- as-ok: vetted\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].message, BASE_MESSAGE);
}

#[test]
fn the_accepted_token_alphabet_is_lowercase_digits_and_plus() {
    // `+` is in the alphabet on purpose: a `n+1`-style rule id derives a `n+1-ok` marker, and an author
    // reaching for it on the WRONG rule must still be disclosed. Uppercase is out — that is a load-bearing
    // half of the prose defense, so both directions are pinned here.
    let plus = scan_pack(
        &near_miss_line_pack(),
        "f.ts",
        "const x = widen(y); // n+1-ok: batched\n",
        vec![],
    );
    assert_eq!(plus.len(), 1, "{plus:?}");
    assert!(
        plus[0].message.contains("reads `n+1-ok`"),
        "`+` must be in the token alphabet: {}",
        plus[0].message
    );

    let upper = scan_pack(
        &near_miss_line_pack(),
        "f.ts",
        "const x = widen(y); // NOT-ok: shouting prose\n",
        vec![],
    );
    assert_eq!(upper.len(), 1, "{upper:?}");
    assert_eq!(upper[0].message, BASE_MESSAGE, "uppercase is not a marker");
}

#[test]
fn a_detached_colon_is_a_conservative_miss_and_a_lone_hyphenated_ok_word_is_accused() {
    // Two honesty pins for the doc's shape sentence. (1) The `:` must be ATTACHED — `// as-ok : reason`
    // reads as a terminator to a human but not to the regex, so it goes unreported (conservative miss,
    // same class as `// as-ok reason`). (2) A comment that is ONLY a hyphenated `-ok` word IS accused:
    // by shape it is indistinguishable from a bare marker, so "prose is never accused" is true of a `-ok`
    // word INSIDE a sentence, not of a one-word comment.
    let detached = scan_pack(
        &near_miss_line_pack(),
        "f.ts",
        "const x = widen(y); // as-ok : guaranteed by caller\n",
        vec![],
    );
    assert_eq!(detached.len(), 1, "{detached:?}");
    assert_eq!(detached[0].message, BASE_MESSAGE);

    let lone = scan_pack(
        &near_miss_line_pack(),
        "f.ts",
        "const x = widen(y); // half-ok\n",
        vec![],
    );
    assert_eq!(lone.len(), 1, "{lone:?}");
    assert!(
        lone[0].message.contains("reads `half-ok`"),
        "a one-word `-ok` comment is marker-shaped and IS disclosed: {}",
        lone[0].message
    );
}

#[test]
fn the_hash_leader_is_not_recognized_for_a_line_scan_finding() {
    // Leader parity with suppression, exactly: `#` is a marker leader for the whole-tree io-scan pass
    // only. A `#`-comment never suppresses a line-scan finding, so it must never be disclosed for one
    // either — otherwise the message would blame a comment that was never in the running.
    let f = scan_pack(
        &near_miss_pack("\\\\.py$"),
        "f.py",
        "x = widen(y)  # as-ok: guaranteed by caller\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].message, BASE_MESSAGE);
}

#[test]
fn a_method_scan_near_miss_is_disclosed_too() {
    let src = "async function f(ids) {\n  for (const id of ids) {\n    // batch-ok: elsewhere\n    await t.findOne(id);\n  }\n}\n";
    let f = scan_pack(&marker_method_pack(), "f.ts", src, vec![method("f", 1, 5)]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(
        f[0].message.contains("reads `batch-ok`") && f[0].message.contains("honors is `n+1-ok`"),
        "{}",
        f[0].message
    );
}
