//! `unhandled-promise-use-effect` + `async-handler-no-try` + `as-cast` + comment-skip/test-path exclusion tests (split from `typescript.rs`).

use super::*;

// --- unhandled-promise-use-effect ---

#[test]
fn use_effect_async_callback_is_flagged() {
    let f = analyze(&[(
        "Foo.tsx",
        "import { useEffect } from \"react\";\nexport function Foo() {\n  useEffect(async () => {\n    await load();\n  }, []);\n  return null;\n}\n",
    )]);
    let hits: Vec<_> = f
        .iter()
        .filter(|x| x.rule_id == "typescript/unhandled-promise-use-effect")
        .collect();
    assert_eq!(hits.len(), 1, "{f:?}");
    assert_eq!(hits[0].file, "Foo.tsx");
    assert_eq!(hits[0].line, 3);
}

#[test]
fn synchronous_use_effect_with_inner_async_iife_is_not_flagged() {
    let f = analyze(&[(
        "Ok.tsx",
        "import { useEffect } from \"react\";\nexport function Ok() {\n  useEffect(() => {\n    void load();\n  }, []);\n}\n",
    )]);
    assert!(
        f.iter()
            .all(|x| x.rule_id != "typescript/unhandled-promise-use-effect"),
        "{f:?}"
    );
}

#[test]
fn unhandled_promise_ok_marker_directly_above_use_effect_suppresses_the_finding() {
    let f = analyze(&[(
        "Marked.tsx",
        "import { useEffect } from \"react\";\nexport function M() {\n  // unhandled-promise-ok: one-time bootstrap\n  useEffect(async () => { await x(); }, []);\n}\n",
    )]);
    assert!(
        f.iter()
            .all(|x| x.rule_id != "typescript/unhandled-promise-use-effect"),
        "{f:?}"
    );
}

// --- async-handler-no-try ---

fn async_handler_findings(files: &[(&str, &str)]) -> Vec<Finding> {
    analyze(files)
        .into_iter()
        .filter(|f| f.rule_id == "typescript/async-handler-no-try")
        .collect()
}

#[test]
fn jsx_on_star_handler_async_plus_await_no_try_catch_is_flagged() {
    let f = async_handler_findings(&[(
        "Btn.tsx",
        "export function Btn() {\n  return <button onClick={async () => { await save(); }}>x</button>;\n}\n",
    )]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].file, "Btn.tsx");
}

#[test]
fn async_handler_wrapped_in_try_catch_is_not_detected() {
    let f = async_handler_findings(&[(
        "Safe.tsx",
        "export function Safe() {\n  return <button onClick={async () => { try { await save(); } catch {} }}>x</button>;\n}\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn async_handler_without_await_is_not_detected() {
    let f = async_handler_findings(&[(
        "NoAwait.tsx",
        "export function C() {\n  return <button onClick={async () => { save(); }}>x</button>;\n}\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn async_handler_ok_marker_directly_above_suppresses_the_finding() {
    let f = async_handler_findings(&[(
        "Marked.tsx",
        "export function Btn() {\n  // async-handler-ok: save() never rejects, error boundary catches the rest\n  return <button onClick={async () => { await save(); }}>x</button>;\n}\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

// --- as-cast ---

fn as_cast_findings(files: &[(&str, &str)]) -> Vec<Finding> {
    analyze(files)
        .into_iter()
        .filter(|f| f.rule_id == "typescript/as-cast")
        .collect()
}

#[test]
fn as_cast_usage_is_flagged_on_each_matching_line() {
    let f = as_cast_findings(&[(
        "casts.ts",
        "const el = document.getElementById(\"root\") as unknown as HTMLElement;\nconst val = (window as any).myGlobal;\n",
    )]);
    // One dangerous-cast occurrence per line here (`as unknown as` + `as any`), so 2 lines -> 2 findings.
    assert_eq!(lines_of(&f), vec![1, 2], "{f:?}");
}

#[test]
fn import_alias_as_is_not_counted_as_as_cast() {
    let f = as_cast_findings(&[(
        "aliased.ts",
        "import { useState as useLocalState } from \"react\";\nimport type { FC as FunctionComponent } from \"react\";\nexport const MyComp: FunctionComponent = () => null;\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn as_ok_marker_on_a_line_suppresses_that_as_from_the_count() {
    // `suppress_marker`'s window covers a finding's own line and the single line directly above. The
    // marker on line 1 self-suppresses its own cast; the second cast (line 3) is two lines below, outside
    // that window, so it still counts.
    let f = as_cast_findings(&[(
        "marked.ts",
        "const safe = raw as any; // as-ok: guaranteed by external API\nconst mid = 1;\nconst unsafe = raw2 as any;\n",
    )]);
    assert_eq!(lines_of(&f), vec![3], "{f:?}");
}

#[test]
fn as_const_assertion_is_not_counted_as_as_cast() {
    // `as const` narrows a literal rather than widening/escaping it — the opposite of a cast escape.
    let f = as_cast_findings(&[(
        "consts.ts",
        "const cfg = { a: 1 } as const;\nconst tuple = [1, 2] as const;\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn jsx_as_prop_is_not_counted_but_a_real_cast_still_is() {
    // A polymorphic-component `as` prop (`<Box as="span">`, Mantine/styled-components style) is a
    // JSX attribute, not a TS cast — the `as=` form is excluded. A genuine cast on its own line still
    // fires.
    let f = as_cast_findings(&[(
        "poly.tsx",
        "const a = <Box as=\"span\">hi</Box>;\nconst b = raw as unknown as Size;\n",
    )]);
    assert_eq!(lines_of(&f), vec![2], "{f:?}");
}

#[test]
fn as_unknown_as_cast_is_still_flagged() {
    // A double cast through `unknown` is a genuine cast escape and must still match.
    let f = as_cast_findings(&[("double.ts", "const val = raw as unknown as string;\n")]);
    assert_eq!(lines_of(&f), vec![1], "{f:?}");
}

#[test]
fn bare_as_unknown_without_a_second_as_is_not_flagged() {
    // `line_pattern` requires "as unknown" to be immediately followed by another "as" to fire — a lone
    // `as unknown` widening cast (no double-cast escape hatch) is safe and must not match.
    let f = as_cast_findings(&[("widen.ts", "const y = x as unknown;\n")]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn as_cast_shaped_text_inside_a_full_line_comment_is_not_flagged() {
    let f = as_cast_findings(&[(
        "commented.ts",
        "// this function acts as unknown as a normal helper here\nconst x = 1;\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn any_and_as_cast_combined_are_aggregated_independently() {
    let files = &[(
        "mixed.ts",
        "export function parse(raw: any): string {\n  return (raw as unknown as string).trim();\n}\n",
    )];
    assert_eq!(any_findings(files).len(), 1, "{:?}", any_findings(files));
    assert_eq!(
        as_cast_findings(files).len(),
        1,
        "{:?}",
        as_cast_findings(files)
    );
}

// --- skip_comment_lines + test-path exclusion: test-only mocks/doubles routinely need loose types ---

#[test]
fn async_handler_shape_mentioned_only_in_a_comment_is_not_flagged() {
    let f = async_handler_findings(&[(
        "Btn.tsx",
        "export function Btn() {\n  // onClick={async () => { await save(); }} -- old handler, replaced below\n  return <button onClick={() => {}}>x</button>;\n}\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn as_cast_inside_a_test_fixture_path_is_not_flagged() {
    let f = as_cast_findings(&[(
        "__tests__/mock.ts",
        "export const mock = {} as unknown as UserRecord;\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}
