//! Exercises `rules/dsl/typescript/typescript.json`'s type-safety and unhandled-promise rules end-to-end
//! through `zzop_engine::analyze_tree` against real swc-parsed TypeScript fixtures. See the rule `message`
//! fields for full rationale: `as-cast` excludes import-alias `as` via `LineScan::exclude_pattern`;
//! `async-handler-no-try` vetoes via `MethodScan::absent` when a `try {` appears anywhere in the enclosing
//! symbol span (coarser than "wraps the specific async call"); `no-explicit-any` and `as-cast` both fire
//! one finding per matching *line*, not per occurrence.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, Finding, RulePackDef};
use zzop_engine::{analyze_tree, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent, no `tempfile` dependency).
struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Loads the real `rules/dsl/typescript/typescript.json` from the repo, filtered to just the `typescript` pack.
fn typescript_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "typescript")
        .expect("typescript.json pack present")
}

fn analyze(files: &[(&str, &str)]) -> Vec<Finding> {
    let dir = TempDir::new("zzop-typescript-pack");
    for (rel, content) in files {
        dir.write(rel, content);
    }
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![typescript_pack()],
        ..EngineConfig::default()
    };
    analyze_tree(dir.path(), &cfg).findings
}

fn any_findings(files: &[(&str, &str)]) -> Vec<Finding> {
    analyze(files)
        .into_iter()
        .filter(|f| f.rule_id == "typescript/no-explicit-any")
        .collect()
}

fn lines_of(findings: &[Finding]) -> Vec<u32> {
    let mut v: Vec<u32> = findings.iter().map(|f| f.line).collect();
    v.sort_unstable();
    v
}

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
        "const el = document.getElementById(\"root\") as HTMLElement;\nconst val = (window as any).myGlobal;\n",
    )]);
    // One `as`-cast occurrence per line here, so 2 lines -> 2 findings.
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
        "const safe = raw as string; // as-ok: guaranteed by external API\nconst mid = 1;\nconst unsafe = raw2 as number;\n",
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
        "const a = <Box as=\"span\">hi</Box>;\nconst b = raw as Size;\n",
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
        "export function parse(raw: any): string {\n  return (raw as string).trim();\n}\n",
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
