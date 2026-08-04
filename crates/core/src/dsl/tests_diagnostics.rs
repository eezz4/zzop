//! Rule-skip diagnostics — a rule the evaluator cannot compile must SAY so, not vanish.
//!
//! Pins five matcher families (line-scan, method-scan, symbol-scan, io-scan, call-scan) plus the two
//! invariants that make the channel safe to wire into a user-facing warnings list: a healthy pack emits
//! NOTHING, and one broken rule is reported once even across many files.
//!
//! ⚠ **`literal-scan` has no case here** and this header used to say "every matcher family", which was
//! the same closed-enumeration-behind-a-totality-quantifier defect the v0.29.0 release audit found in
//! the shipped contract descriptions. The gap is partly covered elsewhere — `pack_loader::rule_issues`
//! checks that family's two regex fields on the LOADER path — so an uncompilable `name_pattern` is not
//! wholly silent; what is unpinned is the EVALUATOR-side skip diagnostic. Backlog owns closing it; this
//! header states the real coverage until then rather than claiming the set.

use crate::attributes::AttributeStore;
use crate::finding::Finding;
use crate::ir::{SourceSymbol, SourceSymbolKind};

use super::ir_scan::{eval_pack_io_scan_into, IoScanTreeContext};
use super::test_support::{io_provide, method, rule_pack, symbol};
use super::{eval_pack_into, RuleContext, SourceFile};

/// Evaluates a one-rule pack over `files`, returning `(findings, diagnostics)`.
fn eval(rule_json: &str, files: Vec<SourceFile>) -> (Vec<Finding>, Vec<String>) {
    let pack = rule_pack(rule_json);
    let ctx = RuleContext { files: &files };
    let mut diagnostics = Vec::new();
    let findings = eval_pack_into(&pack, &ctx, &mut diagnostics);
    (findings, diagnostics)
}

fn file(rel: &str, text: &str, symbols: Vec<SourceSymbol>) -> SourceFile {
    SourceFile {
        loop_spans: Vec::new(),
        function_spans: Vec::new(),
        test_spans: Vec::new(),
        call_sites: Vec::new(),

        string_literals: Vec::new(),
        rel: rel.into(),
        text: text.into(),
        symbols,
        io: None,
    }
}

/// Every diagnostic must be actionable: it names the pack-prefixed rule, says the rule was skipped, and
/// points at the validator that catches this before a scan.
fn assert_actionable(message: &str, rule_id: &str, field: &str) {
    assert!(
        message.contains(&format!("rule \"{rule_id}\"")),
        "diagnostic must name the rule id: {message}"
    );
    assert!(
        message.contains(&format!("`{field}`")),
        "diagnostic must name the offending field: {message}"
    );
    assert!(
        message.contains("SKIPPED"),
        "diagnostic must say the rule was skipped: {message}"
    );
    assert_points_at_the_validator(message);
}

/// The validator pointer must name BOTH host dialects, never one.
///
/// This sink is a public embedder channel, so the crate cannot know whether a CLI or an MCP host will
/// render the line — naming one spelling hands the other audience a command it cannot type. Until
/// 2026-08-01 both messages said only `` `zzop validate-rule-pack` ``, and the pin above asserted only
/// the CLI half, so the reword that fixed it could have been undone without a red. Contract 16
/// (`crates/engine/tests/rule_contracts/host_vocabulary.rs`) is the cross-crate guard on the same fact;
/// this is the unit-level pin, and both message shapes below go through it.
fn assert_points_at_the_validator(message: &str) {
    assert!(
        message.contains("`zzop validate-rule-pack <pack.json>`"),
        "diagnostic must point at the validator's CLI spelling: {message}"
    );
    assert!(
        message.contains("`validate_rule_pack` MCP tool"),
        "diagnostic must point at the validator's MCP twin — an MCP host cannot type a subcommand: \
         {message}"
    );
}

#[test]
fn line_scan_bad_pattern_reports_the_rule_id_and_the_regex_error() {
    let (findings, diags) = eval(
        r#"{"id":"bad","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"("}}"#,
        vec![file("a.ts", "const x = (1);\n", vec![])],
    );
    assert!(
        findings.is_empty(),
        "a rule that cannot compile stays inert"
    );
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_actionable(&diags[0], "t/bad", "line_pattern");
    assert!(
        diags[0].contains("regex error:") && diags[0].contains("regex parse error"),
        "diagnostic must carry the underlying regex error: {}",
        diags[0]
    );
}

#[test]
fn a_healthy_pack_produces_no_diagnostics() {
    let (findings, diags) = eval(
        r#"{"id":"ok","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bsetInterval\\s*\\("}}"#,
        vec![file("a.ts", "setInterval(f, 1);\n", vec![])],
    );
    assert_eq!(findings.len(), 1);
    assert!(diags.is_empty(), "{diags:?}");
}

/// Every regex-typed line-scan field, not just `line_pattern` — the defect class is the whole prologue.
#[test]
fn every_line_scan_regex_field_reports_its_own_name() {
    for (field, matcher) in [
        (
            "file_pattern",
            r#"{"type":"line-scan","file_pattern":"(","line_pattern":"x"}"#,
        ),
        (
            "file_exclude_pattern",
            r#"{"type":"line-scan","file_pattern":"\\.ts$","file_exclude_pattern":"(","line_pattern":"x"}"#,
        ),
        (
            "require_file",
            r#"{"type":"line-scan","file_pattern":"\\.ts$","require_file":"(","line_pattern":"x"}"#,
        ),
        (
            "require_file_all",
            r#"{"type":"line-scan","file_pattern":"\\.ts$","require_file_all":["("],"line_pattern":"x"}"#,
        ),
        (
            "require_file_absent",
            r#"{"type":"line-scan","file_pattern":"\\.ts$","require_file_absent":["("],"line_pattern":"x"}"#,
        ),
        (
            "exclude_pattern",
            r#"{"type":"line-scan","file_pattern":"\\.ts$","exclude_pattern":"(","line_pattern":"x"}"#,
        ),
        (
            "any[].pattern",
            r#"{"type":"line-scan","file_pattern":"\\.ts$","any":[{"pattern":"(","label":"l"}]}"#,
        ),
    ] {
        let rule =
            format!(r#"{{"id":"bad","severity":"warning","message":"m","matcher":{matcher}}}"#);
        let (findings, diags) = eval(&rule, vec![file("a.ts", "x\n", vec![])]);
        assert!(findings.is_empty(), "{field}");
        assert_eq!(diags.len(), 1, "{field}: {diags:?}");
        assert_actionable(&diags[0], "t/bad", field);
    }
}

#[test]
fn a_line_scan_rule_with_no_pattern_field_at_all_is_reported_as_malformed() {
    let (_, diags) = eval(
        r#"{"id":"empty","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$"}}"#,
        vec![file("a.ts", "x\n", vec![])],
    );
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(
        diags[0].contains("neither `line_pattern` nor `any`") && diags[0].contains("SKIPPED"),
        "{}",
        diags[0]
    );
    // The `malformed` message is the OTHER shape this module ships and never went through
    // `assert_actionable` (it names no field), so its validator pointer needs its own pin.
    assert_points_at_the_validator(&diags[0]);
}

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

/// The per-file pass calls the evaluator once per file against one accumulator, so a single broken rule
/// must not repeat itself once per file.
#[test]
fn one_broken_rule_is_reported_once_across_many_files() {
    let pack = rule_pack(
        r#"{"id":"bad","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"("}}"#,
    );
    let mut diags = Vec::new();
    for rel in ["a.ts", "b.ts", "c.ts"] {
        let files = vec![file(rel, "x\n", vec![])];
        let ctx = RuleContext { files: &files };
        eval_pack_into(&pack, &ctx, &mut diags);
    }
    assert_eq!(diags.len(), 1, "{diags:?}");
}

/// Behavior is otherwise unchanged: the broken rule is skipped, its healthy pack-mates still fire.
#[test]
fn a_broken_rule_does_not_take_its_pack_mates_down() {
    let pack: super::RulePackDef = serde_json::from_str(
        r#"{"id":"t","framework":"any","rules":[
            {"id":"bad","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"("}},
            {"id":"good","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bsetInterval\\s*\\("}}
        ]}"#,
    )
    .expect("parse pack");
    let files = vec![file("a.ts", "setInterval(f, 1);\n", vec![])];
    let ctx = RuleContext { files: &files };
    let mut diags = Vec::new();
    let findings = eval_pack_into(&pack, &ctx, &mut diags);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule_id, "t/good");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_actionable(&diags[0], "t/bad", "line_pattern");
}
