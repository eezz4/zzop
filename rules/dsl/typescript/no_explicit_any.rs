//! `no-explicit-any` tests (split from `typescript.rs`; shared fixtures live in the crate root).

use super::*;

// --- no-explicit-any ---

#[test]
fn any_type_usage_is_flagged_on_its_line() {
    let f = any_findings(&[(
        "handler.ts",
        "export function process(data: any): any {\n  return data;\n}\n",
    )]);
    // Two `any` occurrences share line 1, but this DSL fires one finding per line, not per occurrence.
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].file, "handler.ts");
    assert_eq!(f[0].line, 1);
}

#[test]
fn clean_file_with_no_any_has_no_findings() {
    let f = any_findings(&[(
        "clean.ts",
        "export function add(a: number, b: number): number {\n  return a + b;\n}\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn any_spread_across_multiple_lines_flags_each_line() {
    let f = any_findings(&[(
        "multi.ts",
        "type Payload = any;\nfunction wrap(x: any, y: any): void {\n  const z: any = x ?? y;\n}\n",
    )]);
    // Line 1's bare `type Payload = any;` alias RHS has no leading `:` before `any`, a documented residual
    // that is NOT flagged; lines 2 and 3 each have a `: any` annotation and still fire.
    assert_eq!(lines_of(&f), vec![2, 3], "{f:?}");
}

#[test]
fn eslint_disable_comment_naming_the_rule_is_not_flagged() {
    // The bare word "any" inside the rule name `no-explicit-any` is not a type position.
    let f = any_findings(&[(
        "disabled.ts",
        "// eslint-disable-next-line @typescript-eslint/no-explicit-any\nexport const x = 1;\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn prose_string_containing_the_word_any_is_not_flagged() {
    // "any" appearing in ordinary prose text is not a type position.
    let f = any_findings(&[(
        "prose.ts",
        "export const helpText = \"Works in any UI, or none.\";\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn as_any_cast_and_any_array_and_generic_any_shapes_are_flagged() {
    let f = any_findings(&[(
        "shapes.ts",
        "const a = (raw as any).value;\nconst b: any[] = [];\nconst c: Array<any> = [];\n",
    )]);
    assert_eq!(lines_of(&f), vec![1, 2, 3], "{f:?}");
}

#[test]
fn any_ok_marker_on_the_same_line_suppresses_the_finding() {
    let f = any_findings(&[(
        "marked.ts",
        "export function process(data: any) { return data; } // any-ok: untyped legacy plugin payload\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn subdirectory_files_are_traversed_recursively() {
    let f = any_findings(&[(
        "features/auth/utils/token.ts",
        "export function decode(raw: any): string {\n  return raw as string;\n}\n",
    )]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].file, "features/auth/utils/token.ts");
}

#[test]
fn node_modules_directory_is_skipped() {
    let f = any_findings(&[("node_modules/lib/index.ts", "export const x: any = 1;\n")]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn only_ts_and_tsx_extensions_are_scanned_js_excluded() {
    let f = any_findings(&[
        ("util.js", "const x = undefined;\n"),
        ("util.ts", "const x: any = undefined;\n"),
    ]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].file, "util.ts");
}

#[test]
fn empty_or_any_free_tsx_file_has_no_findings() {
    let f = any_findings(&[(
        "Button.tsx",
        "export function Button({ label }: { label: string }) {\n  return label;\n}\n",
    )]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn multiple_files_are_scanned_independently() {
    let f = any_findings(&[
        ("a.ts", "const x: any = 1;\n"),
        ("b.ts", "const y = 2 as number;\n"),
        ("c.ts", "export const ok = true;\n"),
    ]);
    let files: Vec<&str> = f.iter().map(|x| x.file.as_str()).collect();
    assert_eq!(files, vec!["a.ts"], "{f:?}");
}
