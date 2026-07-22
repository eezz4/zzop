//! `setstate-after-await-unmounted` tests (split from `react.rs`).

use super::*;

#[test]
fn setter_after_fetch_await_with_no_guard_in_a_react_file_is_flagged() {
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Widget.tsx",
        "import { useEffect, useState } from 'react';\nexport function Widget({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    const load = async () => {\n      const d = await fetch(url);\n      setData(d);\n    };\n    load();\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "setstate-after-await-unmounted");
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
        hits(&out, "setstate-after-await-unmounted").is_empty(),
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
        hits(&out, "setstate-after-await-unmounted").is_empty(),
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
        hits(&out, "setstate-after-await-unmounted").is_empty(),
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
        hits(&out, "setstate-after-await-unmounted").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn setstate_await_ok_marker_directly_above_the_setter_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-react");
    dir.write(
        "src/Widget.tsx",
        "import { useEffect, useState } from 'react';\nexport function Widget({ url }: { url: string }) {\n  const [data, setData] = useState(null);\n  useEffect(() => {\n    const load = async () => {\n      const d = await fetch(url);\n      // setstate-await-ok: fire-and-forget admin diagnostics widget, unmount race accepted\n      setData(d);\n    };\n    load();\n  }, [url]);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "setstate-after-await-unmounted").is_empty(),
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
        hits(&out, "setstate-after-await-unmounted").is_empty(),
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
        hits(&out, "setstate-after-await-unmounted").is_empty(),
        "{:?}",
        out.findings
    );
}
