//! The test-region gate (`eval`'s `TestRegions`) at the interpreter level: one gate applied after every
//! per-file matcher's dispatch, and skipped for a rule that declares `scan_test_regions`. These tests
//! exercise the shapes the end-to-end `crates/engine/tests/analyze_rust_test_spans.rs` cannot reach
//! cheaply — a second matcher type, a second file in the same context, the zero-cost path, and the
//! opt-out's per-RULE granularity.
//!
//! `SourceFile::test_spans` is populated by hand here on purpose. Where the spans COME from is
//! `zzop_parser_rust::extract_test_spans`' claim and its own tests'; what the interpreter DOES with them
//! is this file's, and coupling the two would make a parser change able to turn these green for the
//! wrong reason.

use super::test_support::method;
use super::{eval_pack, RuleContext, RulePackDef, SourceFile};

fn file(rel: &str, text: &str, test_spans: Vec<(u32, u32)>) -> SourceFile {
    SourceFile {
        rel: rel.into(),
        text: text.into(),
        // One single-line method per fixture line. `method-scan` reports at most once per METHOD, so a
        // single body spanning everything would collapse the two trigger lines into one finding and the
        // gate's effect would be unobservable — measured, on the first draft of this file.
        symbols: (1..=4).map(|n| method(&format!("m{n}"), n, n)).collect(),
        io: None,
        loop_spans: Vec::new(),
        function_spans: Vec::new(),
        test_spans,
        call_sites: Vec::new(),

        string_literals: Vec::new(),
    }
}

fn line_scan_pack() -> RulePackDef {
    serde_json::from_str(
        r#"{
            "id": "t",
            "framework": "any",
            "rules": [
                {
                    "id": "needle",
                    "severity": "warning",
                    "message": "m",
                    "matcher": {
                        "type": "line-scan",
                        "file_pattern": "\\.rs$",
                        "line_pattern": "NEEDLE"
                    }
                }
            ]
        }"#,
    )
    .unwrap()
}

fn method_scan_pack() -> RulePackDef {
    serde_json::from_str(
        r#"{
            "id": "t",
            "framework": "any",
            "rules": [
                {
                    "id": "trigger-without-guard",
                    "severity": "warning",
                    "message": "m",
                    "matcher": {
                        "type": "method-scan",
                        "file_pattern": "\\.rs$",
                        "patterns": [{ "pattern": "NEEDLE", "label": "needle" }],
                        "trigger": "needle",
                        "absent": [{ "pattern": "GUARD", "label": "guard" }]
                    }
                }
            ]
        }"#,
    )
    .unwrap()
}

fn lines(pack: &RulePackDef, files: &[SourceFile]) -> Vec<u32> {
    let ctx = RuleContext { files };
    let mut out: Vec<u32> = eval_pack(pack, &ctx).into_iter().map(|f| f.line).collect();
    out.sort_unstable();
    out
}

#[test]
fn a_line_inside_a_span_is_dropped_and_a_line_outside_it_survives() {
    let text = "NEEDLE\nfiller\nNEEDLE\n";
    let files = vec![file("a.rs", text, vec![(3, 3)])];
    assert_eq!(lines(&line_scan_pack(), &files), vec![1]);
}

#[test]
fn with_no_spans_declared_nothing_is_dropped() {
    // The zero-cost path — `TestRegions::build` returns `None` and the findings are untouched. Also the
    // proof that the fixture itself is not the reason anything disappears above.
    let text = "NEEDLE\nfiller\nNEEDLE\n";
    let files = vec![file("a.rs", text, Vec::new())];
    assert_eq!(lines(&line_scan_pack(), &files), vec![1, 3]);
}

#[test]
fn the_gate_applies_to_method_scan_too_not_only_line_scan() {
    // The gate lives in `eval_pack_impl`, after the matcher dispatch, precisely so a matcher type cannot
    // be forgotten. If it moved into `line_scan.rs` this test is what goes red.
    let text = "NEEDLE\nfiller\nNEEDLE\n";
    let all = vec![file("a.rs", text, Vec::new())];
    assert_eq!(
        lines(&method_scan_pack(), &all),
        vec![1, 3],
        "sanity: the method-scan rule fires on both lines when nothing is gated"
    );
    let gated = vec![file("a.rs", text, vec![(3, 3)])];
    assert_eq!(lines(&method_scan_pack(), &gated), vec![1]);
}

#[test]
fn a_span_only_gates_the_file_that_declares_it() {
    // Findings are matched back to their source file BY PATH. A per-file span leaking onto a sibling
    // would silence real code in the file next door, which is the one way this gate could do damage.
    let text = "NEEDLE\n";
    let files = vec![
        file("gated.rs", text, vec![(1, 1)]),
        file("plain.rs", text, Vec::new()),
    ];
    let ctx = RuleContext { files: &files };
    let hits: Vec<String> = eval_pack(&line_scan_pack(), &ctx)
        .into_iter()
        .map(|f| f.file)
        .collect();
    assert_eq!(hits, vec!["plain.rs".to_string()]);
}

#[test]
fn a_span_boundary_is_inclusive_at_both_ends() {
    let text = "NEEDLE\nNEEDLE\nNEEDLE\nNEEDLE\n";
    let files = vec![file("a.rs", text, vec![(2, 3)])];
    assert_eq!(lines(&line_scan_pack(), &files), vec![1, 4]);
}

/// Two rules, one pack, one file, one span — identical matchers, differing only in `scan_test_regions`.
/// The pair is the point: a test that only showed the opt-out firing could be satisfied by a gate that
/// had stopped working altogether, which is the exact defect this flag was introduced to repair. Written
/// as one context so the two answers come out of the SAME pass over the SAME bytes.
fn mixed_pack() -> RulePackDef {
    serde_json::from_str(
        r#"{
            "id": "t",
            "framework": "any",
            "rules": [
                {
                    "id": "gated",
                    "severity": "warning",
                    "message": "m",
                    "matcher": {
                        "type": "line-scan",
                        "file_pattern": "\\.rs$",
                        "line_pattern": "NEEDLE"
                    }
                },
                {
                    "id": "credential",
                    "severity": "critical",
                    "message": "m",
                    "scan_test_regions": true,
                    "matcher": {
                        "type": "line-scan",
                        "file_pattern": "\\.rs$",
                        "line_pattern": "NEEDLE"
                    }
                }
            ]
        }"#,
    )
    .unwrap()
}

#[test]
fn scan_test_regions_exempts_only_the_rule_that_declares_it() {
    let text = "NEEDLE\nfiller\nNEEDLE\n";
    let files = vec![file("a.rs", text, vec![(3, 3)])];
    let ctx = RuleContext { files: &files };
    let mut hits: Vec<(String, u32)> = eval_pack(&mixed_pack(), &ctx)
        .into_iter()
        .map(|f| (f.rule_id, f.line))
        .collect();
    hits.sort();
    assert_eq!(
        hits,
        vec![
            ("t/credential".to_string(), 1),
            ("t/credential".to_string(), 3),
            ("t/gated".to_string(), 1),
        ],
        "the flagged rule must keep its line-3 finding and the unflagged one must still lose its own"
    );
}

/// The opt-out is a RULE field, not a matcher one, so it must work for a matcher kind that is not
/// line-scan. Pinned because the obvious wrong repair — reading the flag inside `line_scan.rs` — passes
/// every test above and fails this one.
#[test]
fn scan_test_regions_is_honored_for_method_scan_as_well() {
    let mut pack = method_scan_pack();
    pack.rules[0].scan_test_regions = true;
    let text = "NEEDLE\nfiller\nNEEDLE\n";
    let files = vec![file("a.rs", text, vec![(3, 3)])];
    assert_eq!(lines(&pack, &files), vec![1, 3]);
}

/// Absent means gated: a pack authored before the field existed keeps the behavior it had. Pinned
/// separately from `with_no_spans_declared_nothing_is_dropped` because that test proves the SPAN side,
/// and this one proves the serde default did not flip.
#[test]
fn a_rule_that_does_not_mention_the_field_is_still_gated() {
    let pack = line_scan_pack();
    assert!(
        !pack.rules[0].scan_test_regions,
        "`scan_test_regions` must default to false — a pack that never heard of the field must not \
         start judging test regions"
    );
    let files = vec![file("a.rs", "NEEDLE\nfiller\nNEEDLE\n", vec![(3, 3)])];
    assert_eq!(lines(&pack, &files), vec![1]);
}
