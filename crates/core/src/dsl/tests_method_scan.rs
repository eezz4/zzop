//! Method-scan matcher tests: the security `cmd-injection` rule (hand-supplied method spans — no Java
//! parser yet), plus the `absent` veto (v2 #1), innermost-span priority (v2 #4), `file_exclude_pattern`
//! (v3), and `require_file_absent` (v4) extensions.

use crate::ir::{SourceSymbol, SourceSymbolKind};

use super::test_support::{method, rule_pack, scan_methods, scan_pack, snippet};
use super::RulePackDef;

// --- cmd-injection ---

#[test]
fn flags_method_that_execs_and_concatenates_dvja_pingaction_pattern() {
    let src = "public class C {\n  private void run() {\n    String[] cmd = { \"/bin/bash\", \"-c\", \"ping \" + getAddress() };\n    Runtime.getRuntime().exec(cmd);\n  }\n}";
    let f = scan_methods(src, vec![method("run", 2, 5)]);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].data.as_ref().unwrap()["method"], "run");
    assert!(snippet(&f[0]).contains("ping"));
}

#[test]
fn does_not_flag_exec_with_constant_command_no_concatenation() {
    let src = "public class C { void r(){ Runtime.getRuntime().exec(\"ls -la\"); } }";
    assert!(scan_methods(src, vec![method("r", 1, 1)]).is_empty());
}

#[test]
fn does_not_flag_string_concatenation_in_a_method_that_never_execs() {
    let src = "public class C { String g(String n){ return \"hello \" + n; } }";
    assert!(scan_methods(src, vec![method("g", 1, 1)]).is_empty());
}

#[test]
fn does_not_pair_an_exec_in_one_method_with_a_concat_in_another_method_scoped() {
    let src = "public class C {\n  void a() { Runtime.getRuntime().exec(\"safe\"); }\n  String b(String x) { return \"msg \" + x; }\n}";
    let f = scan_methods(src, vec![method("a", 2, 2), method("b", 3, 3)]);
    assert!(f.is_empty());
}

#[test]
fn processbuilder_plus_concatenation_is_flagged() {
    let src =
        "public class C { void r(String h){ new ProcessBuilder(\"sh\",\"-c\",\"curl \" + h).start(); } }";
    let f = scan_methods(src, vec![method("r", 1, 1)]);
    assert_eq!(f.len(), 1);
}

// --- extension #1: method-scan `absent` labels ---

fn toctou_like_pack() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bfindOne\\(","label":"read"},{"pattern":"\\bcreate\\(","label":"write"}],"trigger":"write","absent":[{"pattern":"\\btry\\s*\\{","label":"guard"}]}}"#,
    )
}

#[test]
fn absent_label_does_not_veto_when_no_guard_present() {
    let src = "async function f() {\n  const x = await t.findOne();\n  if (!x) {\n    await t.create();\n  }\n}\n";
    let f = scan_pack(&toctou_like_pack(), "f.ts", src, vec![method("f", 1, 6)]);
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn absent_label_vetoes_finding_when_guard_present_in_same_span() {
    let src = "async function f() {\n  const x = await t.findOne();\n  if (!x) {\n    try {\n      await t.create();\n    } catch (e) {}\n  }\n}\n";
    let f = scan_pack(&toctou_like_pack(), "f.ts", src, vec![method("f", 1, 8)]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn absent_label_only_vetoes_within_the_same_span_not_a_sibling_symbol() {
    // The guard lives in a different function body, so this must still fire.
    let src = "async function f() {\n  const x = await t.findOne();\n  await t.create();\n}\nfunction g() {\n  try {\n  } catch (e) {}\n}\n";
    let f = scan_pack(
        &toctou_like_pack(),
        "f.ts",
        src,
        vec![method("f", 1, 3), method("g", 5, 7)],
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

// --- extension #4: method-scan innermost-span priority ---

fn call_scan_pack() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bfoo\\(","label":"call"}],"trigger":"call"}}"#,
    )
}

#[test]
fn overlapping_class_and_method_spans_are_evaluated_only_at_the_innermost_span() {
    // Mirrors the real TS parser: a class symbol's span covers the whole class, and each method also
    // gets its own nested sub-symbol span. Without extension #4 this would double-count.
    let src = "class C {\n  method() {\n    foo();\n  }\n}\n";
    let outer = SourceSymbol {
        id: "f.ts#C".into(),
        file: "f.ts".into(),
        name: "C".into(),
        kind: SourceSymbolKind::Class,
        line: 1,
        exported: false,
        is_default: false,
        body_start: Some(1),
        body_end: Some(5),
        write_sites: Vec::new(),
    };
    let inner = method("C.method", 2, 4);
    let f = scan_pack(&call_scan_pack(), "f.ts", src, vec![outer, inner]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 3);
    assert_eq!(f[0].data.as_ref().unwrap()["method"], "C.method");
}

#[test]
fn non_overlapping_sibling_spans_are_each_still_evaluated() {
    let src = "function a() {\n  foo();\n}\nfunction b() {\n  foo();\n}\n";
    let f = scan_pack(
        &call_scan_pack(),
        "f.ts",
        src,
        vec![method("a", 1, 3), method("b", 4, 6)],
    );
    assert_eq!(f.len(), 2, "{f:?}");
}

// --- DSL v3 extension: `file_exclude_pattern` (method-scan) ---

fn exclude_pack_method_scan() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","file_exclude_pattern":"(^|/)scripts/","patterns":[{"pattern":"\\bfoo\\(","label":"call"}],"trigger":"call"}}"#,
    )
}

#[test]
fn file_exclude_pattern_skips_a_matching_file_entirely_for_method_scan() {
    let src = "function a() {\n  foo();\n}\n";
    let f = scan_pack(
        &exclude_pack_method_scan(),
        "scripts/build.ts",
        src,
        vec![method("a", 1, 3)],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn file_exclude_pattern_still_flags_a_non_matching_file_for_method_scan() {
    let src = "function a() {\n  foo();\n}\n";
    let f = scan_pack(
        &exclude_pack_method_scan(),
        "src/a.ts",
        src,
        vec![method("a", 1, 3)],
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

// --- MethodScan `require_file_absent` (mirrors LineScan's, see field doc) ---

fn method_scan_require_file_absent_pack() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","require_file_absent":["process\\.on\\s*\\(\\s*['\"]SIG"],"patterns":[{"pattern":"process\\.exit\\s*\\(","label":"exit"}],"trigger":"exit"}}"#,
    )
}

#[test]
fn method_scan_require_file_absent_fires_when_the_absent_pattern_is_missing() {
    let src = "export function shutdown() {\n  process.exit(1);\n}\n";
    let f = scan_pack(
        &method_scan_require_file_absent_pack(),
        "f.ts",
        src,
        vec![method("shutdown", 1, 3)],
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn method_scan_require_file_absent_skips_the_file_when_the_absent_pattern_is_present() {
    let src = "process.on('SIGTERM', () => {\n  process.exit(0);\n});\n";
    let f = scan_pack(
        &method_scan_require_file_absent_pack(),
        "f.ts",
        src,
        vec![method("handler", 1, 3)],
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- trigger-line count (`triggerLines`) ---

/// Two `patterns`, trigger = `call`; no ordering or containment gate, so every matching line counts.
fn count_pack() -> RulePackDef {
    rule_pack(
        r#"{"id":"cnt","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bawait\\b","label":"boundary"},{"pattern":"\\bexec\\s*\\(","label":"call"}],"trigger":"call"}}"#,
    )
}

#[test]
fn a_single_trigger_line_reports_a_count_of_one() {
    let src = "function run() {\n  await x();\n  exec(a);\n}\n";
    let f = scan_pack(&count_pack(), "f.ts", src, vec![method("run", 1, 4)]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].data.as_ref().unwrap()["triggerLines"], 1);
}

#[test]
fn repeated_trigger_lines_in_one_span_are_counted_not_discarded() {
    // One finding per span is the contract and stays; what changes is that the reader can tell a
    // one-off from a method that does the same risky thing four times.
    let src = "function run() {\n  await x();\n  exec(a);\n  exec(b);\n  exec(c);\n  exec(d);\n}\n";
    let f = scan_pack(&count_pack(), "f.ts", src, vec![method("run", 1, 7)]);
    assert_eq!(f.len(), 1, "one finding per span: {f:?}");
    assert_eq!(
        f[0].line, 3,
        "the anchor is still the FIRST qualifying trigger"
    );
    assert_eq!(f[0].data.as_ref().unwrap()["triggerLines"], 4);
}

#[test]
fn a_line_matching_the_trigger_twice_counts_once() {
    // The count is LINES, not match occurrences — the honest reading of what the scan visits.
    let src = "function run() {\n  await x();\n  exec(a); exec(b);\n}\n";
    let f = scan_pack(&count_pack(), "f.ts", src, vec![method("run", 1, 4)]);
    assert_eq!(f[0].data.as_ref().unwrap()["triggerLines"], 1);
}

#[test]
fn a_trigger_line_the_order_gate_rejects_is_not_counted() {
    // Every counted line clears the SAME gates the anchor cleared — a pre-boundary setter neither
    // anchors nor inflates the count.
    let ordered = rule_pack(
        r#"{"id":"cnt2","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bawait\\b","label":"boundary"},{"pattern":"\\bexec\\s*\\(","label":"call"}],"trigger":"call","after":"boundary"}}"#,
    );
    let src = "function run() {\n  exec(before);\n  await x();\n  exec(a);\n  exec(b);\n}\n";
    let f = scan_pack(&ordered, "f.ts", src, vec![method("run", 1, 6)]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 4);
    assert_eq!(f[0].data.as_ref().unwrap()["triggerLines"], 2);
}
