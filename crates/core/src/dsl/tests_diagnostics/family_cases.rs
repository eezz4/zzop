//! The non-line-scan matcher families' skip diagnostics — method-scan, symbol-scan, call-scan,
//! literal-scan, io-scan. Split out of `tests_diagnostics.rs` on 2026-08-08 when the literal-scan
//! pair pushed that file over the line ratchet; the cut is along the seam the file already had (its
//! parent keeps the shared helpers, the line-scan family, and the two channel-wide invariants).

use super::{assert_actionable, assert_points_at_the_validator, eval, file};
use crate::attributes::AttributeStore;
use crate::dsl::ir_scan::{eval_pack_io_scan_into, IoScanTreeContext};
use crate::dsl::test_support::{io_provide, method, rule_pack, symbol};
use crate::ir::SourceSymbolKind;

#[test]
fn method_scan_bad_pattern_and_unknown_trigger_are_both_reported() {
    let src = "void f() {\n  create();\n}\n";
    let symbols = vec![method("f", 1, 3)];
    let (_, diags) = eval(
        r#"{"id":"bad","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"C\\.java$","patterns":[{"pattern":"(","label":"w"}],"trigger":"w"}}"#,
        vec![file("C.java", src, symbols.clone())],
    );
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_actionable(&diags[0], "t/bad", "patterns[].pattern");

    let (_, diags) = eval(
        r#"{"id":"typo","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"C\\.java$","patterns":[{"pattern":"\\bcreate\\(","label":"write"}],"trigger":"wirte"}}"#,
        vec![file("C.java", src, symbols)],
    );
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(
        diags[0].contains("`trigger` names label \"wirte\"") && diags[0].contains("SKIPPED"),
        "{}",
        diags[0]
    );
    assert_points_at_the_validator(&diags[0]);
}

#[test]
fn symbol_scan_bad_name_pattern_is_reported() {
    let (findings, diags) = eval(
        r#"{"id":"bad","severity":"info","message":"m","matcher":{"type":"symbol-scan","file_pattern":"\\.ts$","name_pattern":"("}}"#,
        vec![file(
            "f.ts",
            "",
            vec![symbol("handler", SourceSymbolKind::Function, 1, true)],
        )],
    );
    assert!(findings.is_empty());
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_actionable(&diags[0], "t/bad", "name_pattern");
}

#[test]
fn call_scan_bad_callee_pattern_is_reported() {
    // The site is present and would otherwise match — so the silence here is caused by the skip, not by
    // an empty channel, which is the distinction the diagnostic exists to make visible.
    let mut f = file("f.ts", "console.error('x');\n", Vec::new());
    f.call_sites = vec![crate::CallSite {
        kind: crate::CALL_KIND_CONSOLE_WRITE.to_string(),
        line: 1,
        callee: "console.error".to_string(),
        algorithm: None,
    }];
    let (findings, diags) = eval(
        r#"{"id":"bad","severity":"info","message":"m","matcher":{"type":"call-scan","file_pattern":"\\.ts$","callee_pattern":"("}}"#,
        vec![f],
    );
    assert!(findings.is_empty());
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_actionable(&diags[0], "t/bad", "callee_pattern");
}

#[test]
fn literal_scan_bad_name_pattern_is_reported() {
    // The literal is present and its entropy clears the floor, so the silence below is caused by the
    // SKIP and not by an empty channel — the distinction this diagnostic exists to make visible.
    // Verified by substitution rather than asserted: swapping the broken `(` for `Key$` on this exact
    // fixture produces a finding (`name_pattern` is case-SENSITIVE, so it must be `Key$`, not `key$`).
    let mut f = file("f.ts", "const apiKey = '…';\n", Vec::new());
    f.string_literals = vec![crate::BoundStringLiteral {
        name: "apiKey".to_string(),
        line: 1,
        value_hash: "0123456789abcdef".to_string(),
        entropy: 128.0,
    }];
    let (findings, diags) = eval(
        r#"{"id":"bad","severity":"info","message":"m","matcher":{"type":"literal-scan","file_pattern":"\\.ts$","name_pattern":"(","entropy_min":80}}"#,
        vec![f],
    );
    assert!(findings.is_empty());
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_actionable(&diags[0], "t/bad", "name_pattern");
}

#[test]
fn literal_scan_bad_name_exclude_pattern_is_reported() {
    // The second of this family's two regex fields. Pinned separately because a skip diagnostic that
    // names the wrong field sends the reader to a healthy line — the same reason
    // `every_line_scan_regex_field_reports_its_own_name` exists for line-scan.
    let mut f = file("f.ts", "const apiKey = '…';\n", Vec::new());
    f.string_literals = vec![crate::BoundStringLiteral {
        name: "apiKey".to_string(),
        line: 1,
        value_hash: "0123456789abcdef".to_string(),
        entropy: 128.0,
    }];
    let (findings, diags) = eval(
        r#"{"id":"bad","severity":"info","message":"m","matcher":{"type":"literal-scan","file_pattern":"\\.ts$","name_pattern":"Key$","name_exclude_pattern":"(","entropy_min":80}}"#,
        vec![f],
    );
    assert!(findings.is_empty());
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_actionable(&diags[0], "t/bad", "name_exclude_pattern");
}

#[test]
fn io_scan_bad_key_pattern_is_reported() {
    let pack = rule_pack(
        r#"{"id":"bad","severity":"info","message":"m","matcher":{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","key_pattern":"("}}"#,
    );
    let provides = vec![io_provide("http", "GET /users", 3)];
    let attrs = AttributeStore::from_attrs(Vec::new());
    let ctx = IoScanTreeContext {
        provides: &provides,
        consumes: &[],
        attrs: &attrs,
        anchor_line: &|_file: &str, _line: u32| None,
    };
    let mut out = Vec::new();
    let mut diags = Vec::new();
    eval_pack_io_scan_into(&pack, &ctx, &mut out, &mut diags);
    assert!(out.is_empty(), "a rule that cannot compile stays inert");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_actionable(&diags[0], "t/bad", "key_pattern");
}
