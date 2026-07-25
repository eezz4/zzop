//! `MethodScan::after` — the lexical-ORDER gate on the trigger (`MethodScan::after`'s doc).
//!
//! Sibling of `trigger_in_loop`'s containment gate. Two things are pinned here: the trigger only counts
//! when the ordering label precedes it textually, and the finding's LINE moves to the first trigger that
//! actually follows — the anchor fix that motivated the field (measured: 9 of a 15-finding
//! `react/setstate-after-await-unmounted` sample anchored on a setter before the first `await`).

use super::test_support::{method, rule_pack, scan_pack};
use super::RulePackDef;

/// `boundary` = `await` or `.then(`, `call` = a `setX(` state setter — the react shape, reduced.
/// Without `after` this is a pure co-occurrence matcher; `with_after` adds the order gate.
fn pack(with_after: bool) -> RulePackDef {
    let after = if with_after {
        r#","after":"boundary""#
    } else {
        ""
    };
    rule_pack(&format!(
        r#"{{"id":"ord","severity":"warning","message":"m","matcher":{{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{{"pattern":"\\bawait\\b|\\.then\\s*\\(","label":"boundary"}},{{"pattern":"(?:^|[^.\\w$])set[A-Z][\\w$]*\\s*\\(","label":"call"}}],"trigger":"call"{after}}}}}"#
    ))
}

#[test]
fn without_after_the_anchor_is_the_first_trigger_even_when_it_precedes_the_boundary() {
    // The defect, reproduced: `setA(` on line 2 comes BEFORE the await on line 3, yet it is the reported
    // line. This is the baseline the gate changes — kept as a test so the delta is visible, not asserted
    // about in prose only.
    let src = "async function load() {\n  setA(1);\n  const r = await f();\n  setB(r);\n}\n";
    let f = scan_pack(&pack(false), "f.ts", src, vec![method("load", 1, 5)]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2, "co-occurrence anchors on the first trigger");
}

#[test]
fn after_moves_the_anchor_to_the_first_trigger_that_follows_the_boundary() {
    let src = "async function load() {\n  setA(1);\n  const r = await f();\n  setB(r);\n}\n";
    let f = scan_pack(&pack(true), "f.ts", src, vec![method("load", 1, 5)]);
    assert_eq!(
        f.len(),
        1,
        "the finding still fires — this is a re-anchor, not a veto: {f:?}"
    );
    assert_eq!(f[0].line, 4, "the pre-boundary setter no longer anchors");
    assert_eq!(
        f[0].data.as_ref().unwrap()["snippet"].as_str().unwrap(),
        "setB(r);",
        "the snippet follows the anchor"
    );
}

#[test]
fn after_vetoes_the_span_when_every_trigger_precedes_the_boundary() {
    // The scope/order false-positive class: a setter that runs before the only await proves nothing about
    // running after it. With no qualifying trigger the rule is silent rather than anchoring on a lie.
    let src = "async function load() {\n  setA(1);\n  setB(2);\n  await f();\n}\n";
    let f = scan_pack(&pack(true), "f.ts", src, vec![method("load", 1, 5)]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_boundary_earlier_on_the_same_line_counts_so_a_one_liner_continuation_fires() {
    // `p.then(r => setX(r))` — the continuation callback is the classic unmount race, and it lives on one
    // line. Same-line ordering is decided by start offset: `.then(` precedes `setX(`.
    let src = "function mount() {\n  void load().then((r) => setData(r));\n}\n";
    let f = scan_pack(&pack(true), "f.ts", src, vec![method("mount", 1, 3)]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn a_boundary_later_on_the_same_line_does_not_count() {
    // The mirror of the case above — offset order is what decides, not mere same-line co-presence.
    let src = "async function load() {\n  setA(await f());\n}\n";
    let f = scan_pack(&pack(true), "f.ts", src, vec![method("load", 1, 3)]);
    assert!(
        f.is_empty(),
        "`setA(await f())` has the setter text FIRST — an accepted under-report, pinned: {f:?}"
    );
}

#[test]
fn without_after_the_matcher_is_byte_identical_to_before() {
    // The field is opt-in: a pack that does not set it must evaluate exactly as it always did, including
    // the first-trigger anchor. Guards every shipped method-scan rule that has not opted in.
    let src = "async function load() {\n  setA(1);\n  await f();\n  setB(2);\n}\n";
    let a = scan_pack(&pack(false), "f.ts", src, vec![method("load", 1, 5)]);
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(a[0].line, 2);
    assert_eq!(a[0].message, "m");
}

#[test]
fn after_naming_an_undeclared_label_skips_the_rule_and_reports_it() {
    // Same contract as `trigger`: a typo'd label is malformed, not a silent degrade to co-occurrence —
    // silently dropping the gate would put back exactly the weakness the field removes.
    let pack = rule_pack(
        r#"{"id":"ord","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bawait\\b","label":"boundary"},{"pattern":"\\bsetA\\(","label":"call"}],"trigger":"call","after":"bounadry"}}"#,
    );
    let src = "async function load() {\n  await f();\n  setA(1);\n}\n";
    let f = scan_pack(&pack, "f.ts", src, vec![method("load", 1, 4)]);
    assert!(f.is_empty(), "a malformed rule is skipped: {f:?}");
}

#[test]
fn after_is_evaluated_per_span_so_a_sibling_functions_boundary_does_not_qualify() {
    // method-scan spans are per-symbol, and `after` inherits that: the boundary must precede the trigger
    // WITHIN the same span. Two sibling functions, boundary in the second, trigger in the first.
    let src = "function sync() {\n  setA(1);\n}\nasync function other() {\n  await f();\n}\n";
    let f = scan_pack(
        &pack(true),
        "f.ts",
        src,
        vec![method("sync", 1, 3), method("other", 4, 6)],
    );
    assert!(f.is_empty(), "{f:?}");
}
