//! `MethodScan::after_in_same_function` — the structural PAIRING gate on `after`.
//!
//! Sibling of `tests_method_scan_after` (the ORDER gate it refines) and `tests_trigger_in_loop` (the
//! other parser-fact-backed gate). What is pinned here: an ordering match in a SIBLING closure no longer
//! qualifies a trigger, a promise continuation whose `.then(` was merged into its span still does, and a
//! file with no projected `function_spans` degrades to a NO-OP rather than to silence.
//!
//! The `function_spans` values are hand-supplied here exactly as `tests_trigger_in_loop` hand-supplies
//! `loop_spans`; `zzop_parser_typescript::function_spans` owns the tests for how they are derived from
//! source (including the `.then`/`.catch`/`.finally` merge itself).

use super::test_support::{method, rule_pack, scan_pack, scan_pack_fns};
use super::RulePackDef;

/// The react shape reduced: `boundary` = `await` or `.then(`, `call` = a `setX(` state setter.
fn pack(gated: bool) -> RulePackDef {
    let gate = if gated {
        r#","after_in_same_function":true"#
    } else {
        ""
    };
    rule_pack(&format!(
        r#"{{"id":"pair","severity":"warning","message":"m","matcher":{{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{{"pattern":"\\bawait\\b|\\.then\\s*\\(","label":"boundary"}},{{"pattern":"(?:^|[^.\\w$])set[A-Z][\\w$]*\\s*\\(","label":"call"}}],"trigger":"call","after":"boundary"{gate}}}}}"#
    ))
}

/// The measured false-positive class, reduced. One declared symbol (`Widget`, lines 1-6) holds two
/// unrelated closures: an async submit handler that awaits, and a change handler that sets state. They
/// share the symbol body span, so plain `after` pairs them.
const SIBLING_CLOSURES: &str = "function Widget() {\n  const submit = async () => {\n    await post();\n  };\n  const onChange = (v) => {\n    setValue(v);\n  };\n}\n";
/// `Widget` 1-8, `submit` 2-4, `onChange` 5-7 — what the parser projects for `SIBLING_CLOSURES`.
fn sibling_spans() -> Vec<(u32, u32)> {
    vec![(1, 8), (2, 4), (5, 7)]
}

#[test]
fn ungated_a_sibling_closures_boundary_still_pairs_with_the_trigger() {
    // The baseline the gate changes, kept visible: `after` alone is satisfied by the sibling's `await`.
    let f = scan_pack_fns(
        &pack(false),
        "f.ts",
        SIBLING_CLOSURES,
        vec![method("Widget", 1, 8)],
        sibling_spans(),
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 6);
}

#[test]
fn gated_a_sibling_closures_boundary_no_longer_pairs_with_the_trigger() {
    let f = scan_pack_fns(
        &pack(true),
        "f.ts",
        SIBLING_CLOSURES,
        vec![method("Widget", 1, 8)],
        sibling_spans(),
    );
    assert!(
        f.is_empty(),
        "the await is in `submit`, the setter in `onChange` — no continuation resumes into the other: {f:?}"
    );
}

#[test]
fn gated_a_boundary_in_the_triggers_own_function_still_pairs() {
    // The recall half: same file shape, but the await and the setter are in ONE closure.
    let src = "function Widget() {\n  const load = async () => {\n    const d = await get();\n    setData(d);\n  };\n}\n";
    let f = scan_pack_fns(
        &pack(true),
        "f.ts",
        src,
        vec![method("Widget", 1, 6)],
        vec![(1, 6), (2, 5)],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 4);
}

#[test]
fn gated_a_merged_then_continuation_survives_because_its_span_starts_on_the_then_line() {
    // The whole reason this needs a parser fact. `.then(` is on line 3 and `setFx` on line 4; a naive
    // "nearest function" partition would put line 3 in the `useEffect` callback and line 4 in the
    // continuation, splitting the pair. The projected span for the continuation is (3, 5) — MERGED up to
    // the `.then` line — so both resolve to the same innermost span.
    let src = "function useFx() {\n  useEffect(() => {\n    loadRates().then((d) => {\n      setFx(d);\n    });\n  }, []);\n}\n";
    let f = scan_pack_fns(
        &pack(true),
        "f.ts",
        src,
        vec![method("useFx", 1, 7)],
        vec![(1, 7), (2, 6), (3, 5)],
    );
    assert_eq!(f.len(), 1, "the merged continuation must survive: {f:?}");
    assert_eq!(f[0].line, 4);
}

#[test]
fn gated_the_same_continuation_would_fragment_without_the_merge() {
    // The counter-test that proves the previous one is about the MERGE and not about line numbers: feed
    // the identical source with the callback's UNMERGED span (4, 5) and the pair breaks.
    let src = "function useFx() {\n  useEffect(() => {\n    loadRates().then((d) => {\n      setFx(d);\n    });\n  }, []);\n}\n";
    let f = scan_pack_fns(
        &pack(true),
        "f.ts",
        src,
        vec![method("useFx", 1, 7)],
        vec![(1, 7), (2, 6), (4, 5)],
    );
    assert!(
        f.is_empty(),
        "unmerged, the `.then(` on line 3 sits in the useEffect callback, not the continuation: {f:?}"
    );
}

#[test]
fn gated_a_one_liner_continuation_still_fires_since_same_line_is_trivially_same_span() {
    let src = "function useFx() {\n  void load().then((r) => setData(r));\n}\n";
    let f = scan_pack_fns(
        &pack(true),
        "f.ts",
        src,
        vec![method("useFx", 1, 3)],
        vec![(1, 3), (2, 2)],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn gated_a_later_boundary_in_the_right_function_qualifies_even_after_an_earlier_wrong_one() {
    // The gate tracks the LATEST ordering match, not the first: the sibling's `await` on line 3 must not
    // "use up" the label and hide the real one on line 6.
    let src = "function Widget() {\n  const other = async () => {\n    await post();\n  };\n  const load = async () => {\n    const d = await get();\n    setData(d);\n  };\n}\n";
    let f = scan_pack_fns(
        &pack(true),
        "f.ts",
        src,
        vec![method("Widget", 1, 9)],
        vec![(1, 9), (2, 4), (5, 8)],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 7);
}

#[test]
fn an_enclosing_functions_own_await_still_qualifies_when_a_merged_span_covers_its_line() {
    // Regression from the mono-hub re-measurement: the gate first compared span IDENTITY, which lost
    // this real true positive. Line 2 holds BOTH `warmupAsync`'s own `await` and a `.then` continuation
    // whose merged span covers only that line, so `innermost(2)` is the continuation, not `warmupAsync`
    // — yet the setter on line 3 genuinely runs on `warmupAsync`'s resumed continuation. Containment
    // ("line 2 is inside warmupAsync's span") is the right test; identity is not.
    let src = "async function warmupAsync() {\n  await import(\"./m\").then((m) => m.get());\n  setOcrState(\"ready\");\n}\n";
    let f = scan_pack_fns(
        &pack(true),
        "f.ts",
        src,
        vec![method("warmupAsync", 1, 4)],
        vec![(1, 4), (2, 2)],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 3);
}

#[test]
fn a_setter_whose_own_line_opens_an_inline_callback_re_anchors_rather_than_vanishing() {
    // The accepted LINE-granularity imprecision, measured on mono-hub. `setFiles((prev) => ...)` on
    // line 3 is itself in the continuation, but its line also OPENS an arrow, so line 3 resolves to
    // that arrow's span (3, 3) and the await on line 2 falls outside it. The finding is not lost — it
    // re-anchors on the next plain setter in the same function.
    let src = "async function onAdd() {\n  const entries = await load();\n  setFiles((prev) => [...prev, ...entries]);\n  setUrl(\"\");\n}\n";
    let f = scan_pack_fns(
        &pack(true),
        "f.ts",
        src,
        vec![method("onAdd", 1, 5)],
        vec![(1, 5), (3, 3)],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(
        f[0].line, 4,
        "anchor moves by one line, the finding survives"
    );
}

#[test]
fn no_projected_function_spans_degrades_to_a_no_op_not_to_silence() {
    // Graceful degrade, and its DIRECTION is the point: a parser that produces no function spans leaves
    // the rule exactly as loud as it was without the gate (compare with `trigger_in_loop`, which goes
    // silent). Precision is lost, recall is not.
    let gated = scan_pack_fns(
        &pack(true),
        "f.ts",
        SIBLING_CLOSURES,
        vec![method("Widget", 1, 8)],
        Vec::new(),
    );
    let ungated = scan_pack(
        &pack(false),
        "f.ts",
        SIBLING_CLOSURES,
        vec![method("Widget", 1, 8)],
    );
    assert_eq!(gated.len(), 1, "{gated:?}");
    assert_eq!(gated[0].line, ungated[0].line);
}

#[test]
fn the_gate_is_inert_without_after() {
    // `after_in_same_function` scopes a PAIRING; with no ordering label there is no pair to scope, so the
    // rule must stay a plain co-occurrence matcher rather than silently gaining a second gate.
    let pack = rule_pack(
        r#"{"id":"pair","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bawait\\b","label":"boundary"},{"pattern":"(?:^|[^.\\w$])set[A-Z][\\w$]*\\s*\\(","label":"call"}],"trigger":"call","after_in_same_function":true}}"#,
    );
    let f = scan_pack_fns(
        &pack,
        "f.ts",
        SIBLING_CLOSURES,
        vec![method("Widget", 1, 8)],
        sibling_spans(),
    );
    assert_eq!(
        f.len(),
        1,
        "co-occurrence across sibling closures still fires without `after`: {f:?}"
    );
}
