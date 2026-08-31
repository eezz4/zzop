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
    // `method_scan.rs`). The file HAS projected function spans — the `.then` continuation callback is
    // one — but the scanned symbol is the CLASS: function-VALUED properties leaf out since 2026-08-09
    // (parser `symbol_shapes::emit_class`), so a class-wide span survives only when the class declares
    // no method/constructor/function-property sub-symbol at all, which is the case here — both members
    // are CALL-valued initializers the projection has no function body for. The `reset` line sits
    // inside no function span, resolves to "no enclosing function", and the gate reads that as NO
    // GATE, not as "no pair": the `.then(` boundary on line 3 still pairs with `setColor(` on line 4,
    // exactly as before the gate existed. Deliberate direction — a missing span is absence of
    // evidence, so degrading toward the pre-gate over-report beats inventing an under-report nobody
    // measured. (Until 2026-08-09 this fixture spelled the boundary side as an ARROW property; that
    // spelling now projects its own leaf and its cross-member pairing was exactly the span-boundary
    // FP class — `cases/trees/api-be/spans/` — that this pin must not bless.)
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Panel.tsx",
        "import { useState } from 'react';\nexport class Panel {\n  seed = fetchRates().then((d) => d.json());\n  reset = setColor(1);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

// --- the LIVE-flag veto: a cancellation flag that is actually SET ---
//
// The original guard vocabulary was a list of exact identifier spellings, and it was case-SENSITIVE:
// `cancelled` never matched `isCancelled`, which is the dominant React spelling, and `isDestroyed` was
// not in the list at all. Measured on getredash/redash @ ca79fe98, 11 of this rule's 35 findings sat in
// a file carrying an `isCancelled`/`isDestroyed` spelling. That 11 is a count of CANDIDATE files under
// those two spellings and is NOT the number the fix removed — the removal count is measured over the
// whole arm, which also carries abort/unmount/dispose stems and the `.current` form, and it lives in the
// rule message. The two numbers are stated separately here because a single "11 of 35" restated at both
// sites would read as one measurement and drift the moment either population changed.
// The fix is deliberately NOT "make the old arm case-insensitive": that alone cannot tell a LIVE guard
// from an INERT one, and one of those 11 (`useUserGroups.js:15`) is an inert guard whose finding is
// CORRECT. So the new arm demands the flag be ASSIGNED `true` somewhere in the symbol — the thing a
// cleanup function does and an inert guard never does.

#[test]
fn a_live_is_cancelled_flag_vetoes_while_an_inert_one_still_fires() {
    // The exclusion and its production CONTROL in ONE assertion. Three components, one file:
    //   `Live`      — `let isCancelled = false` + a cleanup that sets it `true`  -> vetoed
    //   `RefGuard`  — a ref flag whose cleanup sets `.current = true`            -> vetoed
    //   `Inert`     — the same flag NAME, read but never set (no cleanup at all) -> still reported
    // A green fixture therefore proves the veto discriminates, not that the scan went dark: if the
    // rule stopped firing entirely, the `Inert` assertion fails.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Guards.tsx",
        "import { useEffect, useRef, useState } from 'react';\ndeclare function fetchIt(u: string): Promise<any>;\nexport function Live({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    let isCancelled = false;\n    fetchIt(url).then((d) => {\n      if (!isCancelled) {\n        setData(d);\n      }\n    });\n    return () => {\n      isCancelled = true;\n    };\n  }, [url]);\n  return null;\n}\nexport function RefGuard({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  const destroyedRef = useRef(false);\n  useEffect(() => {\n    fetchIt(url).then((d) => {\n      if (!destroyedRef.current) {\n        setData(d);\n      }\n    });\n    return () => {\n      destroyedRef.current = true;\n    };\n  }, [url]);\n  return null;\n}\nexport function Inert({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    let isCancelled = false;\n    fetchIt(url).then((d) => {\n      if (!isCancelled) {\n        setData(d);\n      }\n    });\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(
        h.len(),
        1,
        "only the INERT guard may survive the veto: {:?}",
        out.findings
    );
    assert_eq!(
        h[0].line, 39,
        "the survivor must be Inert's setter, not Live's or RefGuard's: {:?}",
        out.findings
    );
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

// --- the `info` BAND, and the gate it stands in for (2026-08-25) ---
//
// The rule's message has always carried a sentence disqualifying its own dominant finding shape: "a
// plain event handler is mounted by construction whenever it fires, so a `setX(...)` inside one is an
// accepted false positive here." A rule that ships a paragraph explaining why its finding is probably
// wrong is describing a gate it has not implemented, and the gate — "the setter sits inside a
// `useEffect` callback, or a function one calls" — is NOT EXPRESSIBLE by `Matcher::MethodScan`:
// `SourceFile` projects function bodies as ANONYMOUS `(start, end)` line pairs (`function_spans`) with
// no record of which CALL receives a function as its argument, `symbols` covers only DECLARED symbols,
// `call_sites` is a closed set of API families with no hook member and answers "does this body contain
// such a call" rather than "is this line inside its callback", and `extract_function_spans` deliberately
// leaves a `useEffect` callback UNMERGED from its call site (its one merge is for
// `.then`/`.catch`/`.finally`, and widening it would re-create the sibling-closure pairing that merge
// exists to break). The second half of the gate — "a function CALLED from an effect" — needs an
// intra-file call graph, which `RuleContext` (per-file, no `CommonIr`) does not carry at all.
//
// So the severity carries the disclosure instead: `info` rather than `warning`. Measured over a 9-tree
// corpus, 34 of 49 findings sat in an event handler rather than an effect.
//
// The two tests below are the BIDIRECTIONAL pin of that state, and the second one is written to be
// INVERTED the day the containment gate lands: today it asserts the false-positive class still fires,
// which is exactly what the band is compensating for.

#[test]
fn an_effect_async_loader_is_the_true_positive_and_reports_at_info() {
    // Direction 1 — RECALL. A genuine mount effect whose async loader sets state after an `await` with
    // no guard. This is the shape the rule exists for, and the demotion must not silence it.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Loader.tsx",
        "import { useEffect, useState } from 'react';\nexport function Loader({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    const load = async () => {\n      const d = await fetch(url);\n      setData(d);\n    };\n    load();\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 7);
    assert_eq!(
        h[0].severity,
        zzop_core::Severity::Info,
        "the band is the un-built gate made machine-readable: {:?}",
        out.findings
    );
}

#[test]
fn an_event_handler_setter_still_fires_and_that_is_what_the_info_band_pays_for() {
    // Direction 2 — the FALSE-POSITIVE class, pinned as PRESENT rather than as removed. The component is
    // mounted by construction whenever `onSubmit` runs, so no teardown can race the resolve; the rule
    // reports anyway, because it cannot see that the setter is in a handler rather than an effect. This
    // fixture is the measured cal.com shape (a `setX(false)` in a `finally`/`catch` reached only from a
    // submit path). INVERT THIS TEST — to `is_empty()`, and the sibling above to `Severity::Warning` —
    // in the same change that teaches the matcher hook-callback containment. Until then, asserting the
    // silence this rule's own message promises would be asserting a fiction.
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/PayForm.tsx",
        "import { useState } from 'react';\nexport function PayForm({ url }: { url: string }) {\n  const [paying, setPaying] = useState(false);\n  const onSubmit = async () => {\n    try {\n      await fetch(url, { method: 'POST' });\n    } finally {\n      setPaying(false);\n    }\n  };\n  return <button onClick={onSubmit}>pay</button>;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-async-unguarded");
    assert_eq!(
        h.len(),
        1,
        "the event-handler class is NOT gated today — see this module's header: {:?}",
        out.findings
    );
    assert_eq!(h[0].line, 8);
    assert_eq!(h[0].severity, zzop_core::Severity::Info);
}
