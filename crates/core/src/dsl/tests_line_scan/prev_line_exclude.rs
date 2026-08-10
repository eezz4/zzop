//! `LineScan::prev_line_exclude_pattern` — the one-line-lookback veto for statement continuations.
//! Born from a measured FP on `db/unawaited-write` (2026-08-09): a formatter-wrapped concise arrow
//! body puts the `=>` that returns the promise on the line ABOVE the match, where `exclude_pattern`
//! never looks.

use crate::dsl::test_support::{rule_pack, scan_pack};

/// A minimal continuation-shaped rule: flag `db.create(`, veto when the previous line ends in `=>`.
fn pack() -> crate::dsl::RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan",
        "file_pattern":"\\.ts$","line_pattern":"\\bdb\\.create\\s*\\(",
        "prev_line_exclude_pattern":"=>\\s*$"}}"#,
    )
}

#[test]
fn a_match_whose_previous_line_ends_the_continuation_shape_is_vetoed() {
    let f = scan_pack(
        &pack(),
        "f.ts",
        "const persist = (o: T) =>\n  db.create(o);\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_match_whose_previous_line_is_a_complete_statement_still_fires() {
    let f = scan_pack(&pack(), "f.ts", "doSomething();\ndb.create(o);\n", vec![]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn a_match_on_the_first_line_of_the_file_has_no_predecessor_and_still_fires() {
    // Line 0 has nothing above it — the veto must not read line -1 (or wrap) into a skip.
    let f = scan_pack(&pack(), "f.ts", "db.create(o);\n", vec![]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 1);
}

#[test]
fn the_window_is_exactly_one_line_a_continuation_two_lines_up_is_out_of_sight() {
    // Honest limitation, pinned as behavior: the arrow two lines above (with an argument line
    // between) is NOT seen — the same 1-line window as marker suppression. A rule using this field
    // must disclose that in its message.
    let f = scan_pack(
        &pack(),
        "f.ts",
        "const persist = (o: T) =>\n  wrap(\n  db.create(o));\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "the window must not widen silently: {f:?}");
}

#[test]
fn the_previous_line_is_tested_under_the_same_string_masking_as_the_match_line() {
    // With `strip_string_literals`, a `=>` living only INSIDE a closed string literal on the previous
    // line is masked away and must not veto.
    let masked = rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan",
        "file_pattern":"\\.ts$","line_pattern":"\\bdb\\.create\\s*\\(",
        "prev_line_exclude_pattern":"=>","strip_string_literals":true}}"#,
    );
    let f = scan_pack(&masked, "f.ts", "log('a => b');\ndb.create(o);\n", vec![]);
    assert_eq!(f.len(), 1, "a string-interior => must not veto: {f:?}");
}
