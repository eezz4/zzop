//! MethodScan `trigger_in_loop` tests (structural containment gate, see field doc).

use super::test_support::{method, rule_pack, scan_pack_loops};
use super::RulePackDef;

fn trigger_in_loop_pack() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"warning","message":"Network call issued inside a loop","suppress_marker":"fetch-ok","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bfetch\\s*\\(","label":"network"}],"trigger":"network","trigger_in_loop":true}}"#,
    )
}

#[test]
fn trigger_in_loop_fires_for_a_trigger_line_inside_a_loop_span() {
    let src = "function f(ids) {\n  for (const id of ids) {\n    fetch(url(id));\n  }\n}\n";
    let f = scan_pack_loops(
        &trigger_in_loop_pack(),
        "f.ts",
        src,
        vec![method("f", 1, 5)],
        vec![(2, 4)], // the for-loop's own span, header line included per `loop_spans` doc
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 3);
}

#[test]
fn trigger_in_loop_ignores_a_trigger_outside_the_loop_span_even_with_a_sibling_loop_span_in_the_same_body(
) {
    // Mono-hub REDDIT shape: a one-shot `fetch` sits earlier in the body, and a `.map` callback span
    // exists elsewhere in the same body but never itself contains a `fetch`. Plain co-occurrence (the
    // pre-`trigger_in_loop` approximation) would have fired on this; the containment gate must not.
    let src = "async function f(items) {\n  const data = fetch(url);\n  const a = 1;\n  const b = 2;\n  const result = items.map(function (item) {\n    return item.id;\n  });\n  return { data, result };\n}\n";
    let f = scan_pack_loops(
        &trigger_in_loop_pack(),
        "f.ts",
        src,
        vec![method("f", 1, 9)],
        vec![(5, 7)], // the `.map` callback body span — does not contain line 2's `fetch`
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn trigger_in_loop_with_no_loop_spans_never_fires() {
    // Graceful degrade: a file with no projected loop spans (external parser / lexical fallback) can
    // never satisfy the trigger, mirroring method-scan's skip of files with no symbol spans.
    let src = "function f(ids) {\n  for (const id of ids) {\n    fetch(url(id));\n  }\n}\n";
    let f = scan_pack_loops(
        &trigger_in_loop_pack(),
        "f.ts",
        src,
        vec![method("f", 1, 5)],
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn trigger_in_loop_fires_for_a_single_line_loop_span() {
    let src = "function f(x) {\n  while (x) fetch(url);\n}\n";
    let f = scan_pack_loops(
        &trigger_in_loop_pack(),
        "f.ts",
        src,
        vec![method("f", 1, 3)],
        vec![(2, 2)], // start == end: a loop whose header and body share one line
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn trigger_in_loop_uses_the_second_match_when_the_first_is_outside_the_loop_span() {
    let src = "async function f(ids) {\n  fetch(warmup);\n  for (const id of ids) {\n    fetch(url(id));\n  }\n}\n";
    let f = scan_pack_loops(
        &trigger_in_loop_pack(),
        "f.ts",
        src,
        vec![method("f", 1, 6)],
        vec![(3, 5)],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    // The out-of-loop match on line 2 neither satisfies the trigger nor supplies the finding's line.
    assert_eq!(f[0].line, 4);
}

#[test]
fn trigger_in_loop_absent_defaults_to_false_and_plain_cooccurrence_still_fires() {
    let pack = rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bfetch\\s*\\(","label":"network"}],"trigger":"network"}}"#,
    );
    let src = "function f() {\n  fetch(url);\n}\n";
    let f = scan_pack_loops(&pack, "f.ts", src, vec![method("f", 1, 3)], vec![]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn trigger_in_loop_suppress_marker_above_the_in_loop_trigger_suppresses() {
    let src = "async function f(ids) {\n  for (const id of ids) {\n    // fetch-ok: batched via queue\n    fetch(url(id));\n  }\n}\n";
    let f = scan_pack_loops(
        &trigger_in_loop_pack(),
        "f.ts",
        src,
        vec![method("f", 1, 6)],
        vec![(2, 5)],
    );
    assert!(f.is_empty(), "{f:?}");
}
