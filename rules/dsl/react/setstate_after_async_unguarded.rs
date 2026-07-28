//! `setstate-after-async-unguarded` tests (split from `react.rs`).

use super::*;

#[test]
fn setter_after_fetch_await_with_no_guard_in_a_react_file_is_flagged() {
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Widget.tsx",
        "import { useEffect, useState } from 'react';\nexport function Widget({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    const load = async () => {\n      const d = await fetch(url);\n      setData(d);\n    };\n    load();\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn abort_controller_guard_anywhere_in_the_function_suppresses_the_finding() {
    // NEGATIVE 1 (pin): the same shape, but an `AbortController`/`signal:` guard is present somewhere in
    // the function — the `absent` veto fires and the rule stays silent.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Widget.tsx",
        "import { useEffect, useState } from 'react';\nexport function Widget({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    const controller = new AbortController();\n    const load = async () => {\n      const d = await fetch(url, { signal: controller.signal });\n      setData(d);\n    };\n    load();\n    return () => controller.abort();\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "setstate-after-async-unguarded").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn setter_with_no_await_anywhere_in_the_function_is_not_flagged() {
    // NEGATIVE 2 (pin): `setX(...)` is present but the function never `await`s anything, so the
    // `await` trigger pattern never satisfies and the rule stays silent.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Widget.tsx",
        "import { useState } from 'react';\nexport function Widget() {\n  const [count, setCount] = useState(0);\n  const increment = () => {\n    setCount(count + 1);\n  };\n  return null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "setstate-after-async-unguarded").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn is_mounted_ref_guard_anywhere_in_the_function_suppresses_the_finding() {
    // Same `absent` veto, exercised via the `isMounted`/`mountedRef` vocabulary rather than
    // `AbortController`, to pin that both guard families are recognized.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Widget.tsx",
        "import { useEffect, useState, useRef } from 'react';\nexport function Widget({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  const mountedRef = useRef(true);\n  useEffect(() => {\n    const load = async () => {\n      const d = await fetch(url);\n      if (mountedRef.current) {\n        setData(d);\n      }\n    };\n    load();\n    return () => {\n      mountedRef.current = false;\n    };\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "setstate-after-async-unguarded").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn non_react_file_with_no_react_import_or_hooks_is_not_scanned() {
    // The `require_file` gate scopes this rule to files that look like React (a `useEffect`/`useState`
    // call, or a `from 'react'` import) — a plain async helper with a `setX(...)`-shaped call and no such
    // evidence is never scanned at all, regardless of the co-occurrence pattern.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/store.ts",
        "let data: unknown = null;\nexport async function load(url: string) {\n  const d = await fetch(url);\n  setData(d);\n}\nfunction setData(d: unknown) {\n  data = d;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "setstate-after-async-unguarded").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn setstate_await_ok_marker_directly_above_the_setter_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Widget.tsx",
        "import { useEffect, useState } from 'react';\nexport function Widget({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    const load = async () => {\n      const d = await fetch(url);\n      // zzop-setstate-after-async-unguarded-ok: fire-and-forget admin diagnostics widget, unmount race accepted\n      setData(d);\n    };\n    load();\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "setstate-after-async-unguarded").is_empty(),
        "{:?}",
        out.findings
    );
}

// Regression (opus review, blocking): the `set[A-Z]` trigger matched `setTimeout`/`setInterval` and
// member DOM/Date/storage setters. A self-scheduling poll (await then setTimeout) must NOT be read as a
// state-setter unmount race.
#[test]
fn set_timeout_after_await_is_not_a_state_setter_and_is_not_flagged() {
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Poll.tsx",
        "import { useState } from \"react\";\ndeclare const url: string;\ndeclare function fetch(u: string): Promise<any>;\nexport function usePoll() {\n  const [, setData] = useState(null);\n  async function poll() {\n    const d = await fetch(url);\n    setData(d);\n    setTimeout(poll, 5000);\n  }\n  return poll;\n}\n",
    );
    let out = scan(&dir);
    // setData(d) is a real setter, but the coexisting setTimeout vetoes the finding (accepted
    // under-report) — the important guarantee is that setTimeout ALONE never fires it.
    assert!(
        hits(&out, "setstate-after-async-unguarded").is_empty(),
        "{:?}",
        out.findings
    );
}

// Member-call setters (`localStorage.setItem`, `res.setHeader`, `date.setHours`) are not React state
// setters — the non-member anchor must exclude them.
#[test]
fn local_storage_set_item_after_await_is_not_a_state_setter_and_is_not_flagged() {
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Persist.tsx",
        "import { useEffect } from \"react\";\ndeclare const url: string;\ndeclare function fetch(u: string): Promise<any>;\nexport function usePersist() {\n  useEffect(() => {\n    (async () => {\n      const d = await fetch(url);\n      localStorage.setItem(\"cache\", JSON.stringify(d));\n    })();\n  }, []);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "setstate-after-async-unguarded").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- `after: "async-boundary"` (A5): the ORDER leg, and the `.then(` continuation it made visible ---
//
// Measured on mono-hub (7 trees, 78 baseline findings): 9 of a 15-finding sample anchored on a setter
// that ran BEFORE the first `await` in the whole file, while the rule id claimed "after-await". The
// `after` gate makes the reported line the first setter that provably follows an async boundary.

#[test]
fn the_reported_line_is_the_first_setter_after_the_await_not_the_first_setter_in_the_function() {
    // The anchor defect, end to end. `setStatus` on the line before the await used to be the reported
    // line; the honest anchor is `setData` on the line after it.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Widget.tsx",
        "import { useEffect, useState } from 'react';\nexport function Widget({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    const load = async () => {\n      setStatus('loading');\n      const d = await fetch(url);\n      setData(d);\n    };\n    load();\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(
        h[0].line, 8,
        "must anchor on setData, not the pre-await setStatus"
    );
}

#[test]
fn a_setter_that_only_precedes_the_await_is_no_longer_flagged_at_all() {
    // Pure co-occurrence used to fire here: a setter and an await share the function, but the setter can
    // never run on the resumed continuation. Nothing follows the boundary, so the rule is now silent.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Widget.tsx",
        "import { useEffect, useState } from 'react';\nexport function Widget({ url }: { url: string }) {\n  const [busy, setBusy] = useState(false);\n  useEffect(() => {\n    const load = async () => {\n      setBusy(true);\n      await fetch(url);\n    };\n    load();\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "setstate-after-async-unguarded").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_then_continuation_setting_state_in_a_mount_effect_is_flagged() {
    // The class the rule was structurally blind to: no `await` anywhere, so the old `await`-only pattern
    // never satisfied. This is the highest-value true positive shape — a mount effect whose promise
    // continuation sets state with no guard. Measured: 11 findings of this shape across mono-hub,
    // including the ONE genuine race in the coordinator's 15-finding sample.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/useFxData.tsx",
        "import { useEffect, useState } from 'react';\nexport function useFxData() {\n  const [fx, setFx] = useState(null);\n  useEffect(() => {\n    void fetchRates().then((d) => setFx(d));\n  }, []);\n  return fx;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(
        h[0].line, 5,
        "the one-liner continuation anchors on its own line"
    );
}

// --- `after_in_same_function` (A5 remainder): the SCOPE leg, end to end through the real parser ---
//
// `after` alone pairs across a symbol's whole body span, and a method-scan span is a DECLARED symbol —
// a React component function, closures included. Measured on mono-hub: 4 of a 15-finding sample were a
// setter paired with an unrelated SIBLING closure's `await`. The gate below requires both matches to sit
// in the same innermost `function_spans` entry; the tests here run the real projection, so they also pin
// the parser-side merge (a `.then` callback keeps its call-site line) rather than hand-fed spans.

#[test]
fn a_setter_in_a_sibling_closure_no_longer_pairs_with_another_closures_await() {
    // The false-positive class. `submit` awaits, `onChange` sets state; neither resumes into the other,
    // and no continuation of the await can reach `setValue`.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Widget.tsx",
        "import { useState } from 'react';\nexport function Widget({ url }: { url: string }) {\n  const [value, setValue] = useState('');\n  const submit = async () => {\n    await fetch(url, { method: 'POST' });\n  };\n  const onChange = (v: string) => {\n    setValue(v);\n  };\n  return { submit, onChange };\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "setstate-after-async-unguarded").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_setter_after_an_await_in_its_own_closure_still_fires_under_the_scope_gate() {
    // The recall half of the same shape — the pin that the gate removes PAIRINGS, not the rule. Same file
    // layout as above, but the await and the setter share one closure.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Widget.tsx",
        "import { useEffect, useState } from 'react';\nexport function Widget({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    const load = async () => {\n      const d = await fetch(url);\n      setData(d);\n    };\n    load();\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 7);
}

#[test]
fn a_multiline_then_continuation_survives_the_scope_gate_via_the_parser_merge() {
    // The measured true-positive class the naive "nearest function" scoping destroyed: `.then(` is on
    // line 5 and the setter on line 6, in what a plain partition calls two different functions.
    // `extract_function_spans` merges the callback up to the `.then` line, so both land in one span.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/useFxData.tsx",
        "import { useEffect, useState } from 'react';\nexport function useFxData() {\n  const [fx, setFx] = useState(null);\n  useEffect(() => {\n    fetchRates().then((d) => {\n      setFx(d);\n    });\n  }, []);\n  return fx;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

#[test]
fn a_then_callback_on_its_own_line_below_the_then_token_also_survives() {
    // The merge's widest case: the callback's own first line (6) is BELOW the `.then(` line (5), so only
    // the pulled-up start keeps the boundary inside the trigger's span.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/useFxData.tsx",
        "import { useEffect, useState } from 'react';\nexport function useFxData() {\n  const [fx, setFx] = useState(null);\n  useEffect(() => {\n    fetchRates().then(\n      (d) => {\n        setFx(d);\n      },\n    );\n  }, []);\n  return fx;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 7);
}

#[test]
fn a_then_continuation_with_an_is_mounted_guard_is_still_suppressed() {
    // The `absent` veto is orthogonal to the new boundary — a guarded continuation stays silent.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/useFxData.tsx",
        "import { useEffect, useRef, useState } from 'react';\nexport function useFxData() {\n  const [fx, setFx] = useState(null);\n  const mountedRef = useRef(true);\n  useEffect(() => {\n    void fetchRates().then((d) => {\n      if (mountedRef.current) setFx(d);\n    });\n    return () => {\n      mountedRef.current = false;\n    };\n  }, []);\n  return fx;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "setstate-after-async-unguarded").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_class_property_setter_outside_every_function_span_keeps_the_pre_gate_pairing() {
    // Contract pin for `after_in_same_function`'s PER-LINE degrade (`unwrap_or(0)` in
    // `method_scan.rs`). The file HAS projected function spans — the `load` arrow property is one — but
    // the scanned symbol is the CLASS (a class body span is scanned whenever the class declares no
    // method/constructor sub-symbol, which is the case here: both members are properties), and the
    // `reset` property-initializer line sits inside no function span at all. That line therefore
    // resolves to "no enclosing function", which the gate reads as NO GATE, not as "no pair": the
    // sibling arrow's `await` on line 4 still pairs with `setColor(` on line 6, exactly as before the
    // gate existed. Deliberate direction — a missing span is absence of evidence, so degrading toward
    // the pre-gate over-report beats inventing an under-report nobody measured.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Panel.tsx",
        "import { useState } from 'react';\nexport class Panel {\n  load = async () => {\n    await fetch('/x');\n  };\n  reset = setColor(1);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

// Seals the SAME-LINE NESTING RESIDUAL the rule's message discloses: `setData(await fetch(url));`
// is a genuine unguarded post-await setter that this rule does NOT report, because `order_ok`
// (`crates/core/src/dsl/method_scan.rs`) compares first-match START OFFSETS and the `await` nested
// inside the setter's own argument list starts after `setData(`. The SECOND component in the same
// file writes the identical logic as two statements and DOES fire, so the assertion below proves the
// silence is the nesting, not the fixture.
#[test]
fn an_await_nested_inside_the_setter_call_is_the_disclosed_false_negative() {
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Inline.tsx",
        "import { useEffect, useState } from 'react';\nexport function Inline({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    const load = async () => {\n      setData(await fetch(url));\n    };\n    load();\n  }, [url]);\n  return null;\n}\nexport function TwoStatement({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    const load = async () => {\n      const d = await fetch(url);\n      setData(d);\n    };\n    load();\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 17, "{:?}", out.findings);
}
