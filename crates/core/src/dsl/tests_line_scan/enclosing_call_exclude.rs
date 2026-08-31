//! `LineScan::enclosing_call_exclude_pattern` — the UPWARD veto over the enclosing expression.
//! Born from a measured PRESCRIPTION defect on `db/unawaited-write` (2026-08-21): Prisma's array
//! transaction takes UN-awaited builders as its elements, so the rule's own remedy ("await the call")
//! moves each element OUT of the transaction. The opener sits several lines up, where neither
//! `exclude_pattern` nor either one-line lookback can see it.

use crate::dsl::test_support::{rule_pack, scan_pack};

/// A minimal enclosing-shaped rule: flag `db.create(`, veto when a still-open opener line above the
/// match shows an AWAITED array transaction. The `await` half is not decoration — see
/// `an_opener_whose_own_promise_nobody_consumes_does_not_veto` below.
fn pack() -> crate::dsl::RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan",
        "file_pattern":"\\.ts$","line_pattern":"\\bdb\\.create\\s*\\(",
        "enclosing_call_exclude_pattern":"\\bawait\\b[^\\n]*\\$transaction\\s*\\(\\s*\\["}}"#,
    )
}

#[test]
fn a_match_inside_an_awaited_array_transaction_is_vetoed() {
    let f = scan_pack(
        &pack(),
        "f.ts",
        "await db.$transaction([\n  db.create(a),\n  db.create(b),\n]);\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_match_whose_enclosing_opener_is_an_ordinary_call_still_fires() {
    let f = scan_pack(&pack(), "f.ts", "wrap(\n  db.create(a),\n);\n", vec![]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn a_match_after_the_transaction_group_closed_still_fires() {
    // The walk consumes a group it sees close, so a COMPLETED transaction above says nothing about the
    // bare write below it. This is the "veto went body-scoped" regression, in the smallest possible form.
    let f = scan_pack(
        &pack(),
        "f.ts",
        "await db.$transaction([\n  db.create(a),\n]);\n db.create(b);\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 4);
}

#[test]
fn an_opener_whose_own_promise_nobody_consumes_does_not_veto() {
    // Without `await`/`return`/`yield` on the opener line the writes really ARE unawaited, so the
    // pattern must not match — and the site must keep firing.
    let f = scan_pack(
        &pack(),
        "f.ts",
        "db.$transaction([\n  db.create(a),\n]);\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn a_match_on_the_first_line_of_the_file_has_nothing_above_it_and_still_fires() {
    let f = scan_pack(&pack(), "f.ts", "db.create(a);\n", vec![]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 1);
}

#[test]
fn the_pattern_is_compiled_multiline_so_a_caret_anchors_to_the_opener_line() {
    // The window is several lines joined with `\n`. Compiled with `compile_opt` instead of
    // `compile_opt_multiline`, `^await` here would anchor to the START OF THE WINDOW — which is the
    // OUTERMOST opener, not the one the author is describing — and this veto would silently miss.
    let anchored = rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan",
        "file_pattern":"\\.ts$","line_pattern":"\\bdb\\.create\\s*\\(",
        "enclosing_call_exclude_pattern":"^\\s*await\\b[^\\n]*\\$transaction\\s*\\(\\s*\\["}}"#,
    );
    let f = scan_pack(
        &anchored,
        "f.ts",
        "runAll(\n  await db.$transaction([\n    db.create(a),\n  ]),\n);\n",
        vec![],
    );
    assert!(
        f.is_empty(),
        "`^` must anchor per line, not per window: {f:?}"
    );
}

#[test]
fn the_window_is_tested_under_the_same_string_masking_as_the_match_line() {
    // With `strip_string_literals`, an opener spelled only INSIDE a closed string literal is masked
    // away and must not waive a real finding.
    let masked = rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan",
        "file_pattern":"\\.ts$","line_pattern":"\\bdb\\.create\\s*\\(",
        "enclosing_call_exclude_pattern":"\\bawait\\b[^\\n]*\\$transaction\\s*\\(\\s*\\[",
        "strip_string_literals":true}}"#,
    );
    let f = scan_pack(
        &masked,
        "f.ts",
        "log(\"await db.$transaction([\",\n  db.create(a),\n);\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "a string-interior opener must not veto: {f:?}");
}

#[test]
fn a_bad_regex_skips_the_whole_rule_rather_than_dropping_the_veto() {
    let broken = rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan",
        "file_pattern":"\\.ts$","line_pattern":"\\bdb\\.create\\s*\\(",
        "enclosing_call_exclude_pattern":"("}}"#,
    );
    let f = scan_pack(&broken, "f.ts", "db.create(a);\n", vec![]);
    assert!(f.is_empty(), "{f:?}");
}
