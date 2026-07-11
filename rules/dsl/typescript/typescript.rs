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

// --- float-equality / always-false-comparison / numeric-string-comparison / tofixed-arithmetic /
// date-pitfalls / foreach-async-callback / promise-async-executor / parseint-no-radix ---
//
// These 8 rules are plain-JS correctness bugs, not TS-type-system bugs, so unlike the 4 rules above they
// use the broadened `file_pattern` `(?i)\.(ts|tsx|js|jsx|mjs|cjs)$` — every fixture file below uses a
// `.js`/`.ts` mix on purpose to exercise that breadth.

/// Loads both the `typescript` and `be-db` packs together, for the `float-equality` /
/// `float-money-compare` dual-pack boundary fixture below (be-db.json ships in the same `dsl/` tree).
fn typescript_and_be_db_packs() -> Vec<RulePackDef> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    let mut packs: Vec<RulePackDef> = result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .filter(|p| p.id == "typescript" || p.id == "be-db")
        .collect();
    assert_eq!(packs.len(), 2, "{packs:?}");
    packs.sort_by(|a, b| a.id.cmp(&b.id));
    packs
}

fn analyze_with_packs(files: &[(&str, &str)], packs: Vec<RulePackDef>) -> Vec<Finding> {
    let dir = TempDir::new("zzop-typescript-be-db-pack");
    for (rel, content) in files {
        dir.write(rel, content);
    }
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs,
        ..EngineConfig::default()
    };
    analyze_tree(dir.path(), &cfg).findings
}

fn rule_findings(files: &[(&str, &str)], rule: &str) -> Vec<Finding> {
    analyze(files)
        .into_iter()
        .filter(|f| f.rule_id == format!("typescript/{rule}"))
        .collect()
}

// --- float-equality ---

#[test]
fn float_literal_on_the_right_of_strict_equality_is_flagged() {
    let f = rule_findings(
        &[(
            "calc.ts",
            "export function isDone(ratio: number) {\n  return ratio === 0.1;\n}\n",
        )],
        "float-equality",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn float_literal_on_the_left_of_loose_inequality_is_flagged() {
    let f = rule_findings(
        &[(
            "calc.js",
            "export function notComplete(ratio) {\n  return 0.3 != ratio;\n}\n",
        )],
        "float-equality",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn negative_exponent_float_literal_comparison_is_flagged() {
    let f = rule_findings(
        &[(
            "calc.ts",
            "export function tiny(x: number) {\n  return x === 5e-9;\n}\n",
        )],
        "float-equality",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn integer_literal_strict_equality_is_not_flagged() {
    let f = rule_findings(
        &[(
            "calc.ts",
            "export function isThree(x: number) {\n  return x === 3;\n}\n",
        )],
        "float-equality",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn money_named_float_comparison_fires_only_float_money_compare_not_float_equality() {
    // Boundary: be-db/float-money-compare already owns money-named-identifier-vs-float-literal — this
    // dual-pack fixture proves `price === 19.99` fires exactly one finding total (from be-db), and a
    // non-money-named comparison (`ratio === 0.1`) fires exactly one finding total (from typescript).
    let files: &[(&str, &str)] = &[(
        "money.ts",
        "export function isBasicPlan(price: number, ratio: number) {\n  const a = price === 19.99;\n  const b = ratio === 0.1;\n  return a || b;\n}\n",
    )];
    let f = analyze_with_packs(files, typescript_and_be_db_packs());

    let money_line = 2u32;
    let ratio_line = 3u32;

    let on_money_line: Vec<&Finding> = f.iter().filter(|x| x.line == money_line).collect();
    assert_eq!(on_money_line.len(), 1, "{f:?}");
    assert_eq!(
        on_money_line[0].rule_id, "be-db/float-money-compare",
        "{f:?}"
    );

    let on_ratio_line: Vec<&Finding> = f.iter().filter(|x| x.line == ratio_line).collect();
    assert_eq!(on_ratio_line.len(), 1, "{f:?}");
    assert_eq!(
        on_ratio_line[0].rule_id, "typescript/float-equality",
        "{f:?}"
    );
}

#[test]
fn float_eq_ok_marker_suppresses_the_finding() {
    let f = rule_findings(
        &[(
            "calc.ts",
            "export function isDone(ratio: number) {\n  return ratio === 0.1; // float-eq-ok: tolerance checked elsewhere\n}\n",
        )],
        "float-equality",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- always-false-comparison ---

#[test]
fn nan_strict_equality_is_flagged() {
    let f = rule_findings(
        &[(
            "v.ts",
            "export function isBad(x: number) {\n  return x === NaN;\n}\n",
        )],
        "always-false-comparison",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn number_is_nan_call_is_not_flagged() {
    let f = rule_findings(
        &[(
            "v.ts",
            "export function isBad(x: number) {\n  return Number.isNaN(x);\n}\n",
        )],
        "always-false-comparison",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn empty_array_reference_equality_is_flagged() {
    let f = rule_findings(
        &[(
            "v.js",
            "export function isEmpty(items) {\n  return items === [];\n}\n",
        )],
        "always-false-comparison",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn empty_object_reference_equality_reverse_form_is_flagged() {
    let f = rule_findings(
        &[(
            "v.js",
            "export function isEmptyConfig(config) {\n  return {} === config;\n}\n",
        )],
        "always-false-comparison",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn ordinary_function_body_braces_do_not_false_positive_the_empty_object_reverse_form() {
    // The empty-object reverse pattern (`{}` immediately before an operator) does not collide with a
    // function's closing brace: an operator never immediately follows a block's `}` on the same line in
    // ordinary code.
    let f = rule_findings(
        &[(
            "v.js",
            "function noop() {}\nexport function check(x) {\n  return x === noop();\n}\n",
        )],
        "always-false-comparison",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn loose_equality_with_empty_array_is_not_flagged() {
    // Review honesty pin: loose `x == []` is deliberately OUT of scope — coercion can make it true
    // (`0 == []` is true, `'' == []` is true), so the "constant result" claim only holds for strict
    // `===`/`!==` on the array/object labels. NaN keeps loose coverage (NaN never loose-equals anything).
    let f = rule_findings(
        &[("v.js", "export function check(x) {\n  return x == [];\n}\n")],
        "always-false-comparison",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn always_false_ok_marker_suppresses_the_finding() {
    let f = rule_findings(
        &[(
            "v.ts",
            "export function isBad(x: number) {\n  // always-false-ok: legacy guard, dead code path\n  return x === NaN;\n}\n",
        )],
        "always-false-comparison",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- numeric-string-comparison ---

#[test]
fn numeric_string_on_the_right_of_less_than_is_flagged() {
    let f = rule_findings(
        &[("v.js", "export function cmp(x) {\n  return x < '9';\n}\n")],
        "numeric-string-comparison",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn numeric_string_on_the_left_spaced_greater_than_is_flagged() {
    let f = rule_findings(
        &[("v.js", "export function cmp(x) {\n  return '10' > x;\n}\n")],
        "numeric-string-comparison",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn generic_type_argument_string_literal_before_closing_bracket_is_not_flagged() {
    // `Extract<T, '1'>` — the `<` before the generic's type param is not followed by a quote, and the
    // `'1'>` closing shape has no whitespace before the `>`, so the (deliberately spaced) reverse pattern
    // does not match either.
    let f = rule_findings(
        &[("v.ts", "type A = Extract<T, '1'>;\nexport type { A };\n")],
        "numeric-string-comparison",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn single_arg_generic_numeric_string_literal_is_not_flagged() {
    // Review blocking pin: `useState<'0' | '1'>('0')` — the generic bracket `<` sits directly against
    // the identifier with no space, so the forward pattern's required leading whitespace excludes it.
    let f = rule_findings(
        &[(
            "v.tsx",
            "export function useToggle() {\n  const s = useState<'0' | '1'>('0');\n  return s;\n}\n",
        )],
        "numeric-string-comparison",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn arrow_function_returning_numeric_string_is_not_flagged() {
    // Review blocking pin: the `>` of `=>` is preceded by `=`, not whitespace, so the forward
    // pattern's leading-whitespace requirement excludes arrow returns like `() => '10'`.
    let f = rule_findings(
        &[(
            "v.js",
            "export const version = () => '10';\nexport const zero = (x) => '0';\n",
        )],
        "numeric-string-comparison",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn numeric_string_cmp_ok_marker_suppresses_the_finding() {
    let f = rule_findings(
        &[(
            "v.js",
            "export function cmp(x) {\n  return x < '9'; // numeric-string-cmp-ok: x is itself a formatted string here\n}\n",
        )],
        "numeric-string-comparison",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- tofixed-arithmetic ---

#[test]
fn arithmetic_after_tofixed_result_is_flagged() {
    let f = rule_findings(
        &[(
            "v.js",
            "export function delta(total) {\n  return total.toFixed(2) - 1;\n}\n",
        )],
        "tofixed-arithmetic",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn arithmetic_before_tofixed_result_is_flagged() {
    let f = rule_findings(
        &[(
            "v.js",
            "export function delta(total) {\n  return 1 - total.toFixed(2);\n}\n",
        )],
        "tofixed-arithmetic",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn arithmetic_inside_parens_then_tofixed_is_not_flagged() {
    // Calibration pin (immich server, 10 corpus FPs before the fix): `(finish - start).toFixed(2)` is
    // the CORRECT idiom this rule's own fix guidance recommends — arithmetic first, format last. The
    // before-form's operand class must not include `)`/`]`, or the match crosses the closing paren and
    // flags the good shape.
    let f = rule_findings(
        &[(
            "v.js",
            "export function duration(finish, start) {\n  return (finish - start).toFixed(2);\n}\n",
        )],
        "tofixed-arithmetic",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn string_concatenation_with_tofixed_is_not_flagged() {
    // Scope boundary: `+` is deliberately excluded (display formatting is the common, intended case).
    let f = rule_findings(
        &[(
            "v.js",
            "export function label(n) {\n  return \"a\" + n.toFixed(2);\n}\n",
        )],
        "tofixed-arithmetic",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn tofixed_arith_ok_marker_suppresses_the_finding() {
    let f = rule_findings(
        &[(
            "v.js",
            "export function delta(total) {\n  return total.toFixed(2) - 1; // tofixed-arith-ok: re-quantized intentionally\n}\n",
        )],
        "tofixed-arithmetic",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- date-pitfalls ---

#[test]
fn date_only_iso_string_is_flagged() {
    let f = rule_findings(
        &[("v.ts", "export const d = new Date('2024-01-15');\n")],
        "date-pitfalls",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 1);
}

#[test]
fn ten_digit_seconds_epoch_is_flagged() {
    let f = rule_findings(
        &[("v.ts", "export const d = new Date(1700000000);\n")],
        "date-pitfalls",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 1);
}

#[test]
fn day_ms_literal_alongside_gettime_is_flagged() {
    let f = rule_findings(
        &[(
            "v.ts",
            "export function tomorrow(d: Date) {\n  return new Date(d.getTime() + 86400000);\n}\n",
        )],
        "date-pitfalls",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn day_ms_literal_with_no_date_context_on_the_line_is_not_flagged() {
    let f = rule_findings(
        &[("v.ts", "export const cacheTtlMs = 86400000;\n")],
        "date-pitfalls",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn milliseconds_epoch_thirteen_digits_is_not_flagged() {
    let f = rule_findings(
        &[("v.ts", "export const d = new Date(1700000000000);\n")],
        "date-pitfalls",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn date_pitfall_ok_marker_suppresses_the_finding() {
    let f = rule_findings(
        &[(
            "v.ts",
            "// date-pitfall-ok: server is UTC-only, epoch is confirmed seconds\nexport const d = new Date(1700000000);\n",
        )],
        "date-pitfalls",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- foreach-async-callback ---

#[test]
fn foreach_async_callback_is_flagged() {
    let f = rule_findings(
        &[(
            "v.js",
            "export async function run(items) {\n  items.forEach(async (item) => {\n    await save(item);\n  });\n}\n",
        )],
        "foreach-async-callback",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn map_async_callback_does_not_fire_the_foreach_rule() {
    // Sibling boundary: `.map(async ...)` is a different defect owned by be-reliability/await-in-map.
    let f = rule_findings(
        &[(
            "v.js",
            "export async function run(items) {\n  return items.map(async (item) => {\n    return save(item);\n  });\n}\n",
        )],
        "foreach-async-callback",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn foreach_async_ok_marker_suppresses_the_finding() {
    let f = rule_findings(
        &[(
            "v.js",
            "export async function run(items) {\n  items.forEach(async (item) => { // foreach-async-ok: fire-and-forget by design\n    await save(item);\n  });\n}\n",
        )],
        "foreach-async-callback",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- promise-async-executor ---

#[test]
fn async_promise_executor_is_flagged() {
    let f = rule_findings(
        &[(
            "v.js",
            "export function wrap() {\n  return new Promise(async (resolve, reject) => {\n    resolve(await load());\n  });\n}\n",
        )],
        "promise-async-executor",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn synchronous_promise_executor_is_not_flagged() {
    let f = rule_findings(
        &[(
            "v.js",
            "export function wrap() {\n  return new Promise((resolve, reject) => {\n    load().then(resolve, reject);\n  });\n}\n",
        )],
        "promise-async-executor",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn promise_async_exec_ok_marker_suppresses_the_finding() {
    let f = rule_findings(
        &[(
            "v.js",
            "export function wrap() {\n  // promise-async-exec-ok: rejections are handled by the caller's catch\n  return new Promise(async (resolve) => { resolve(await load()); });\n}\n",
        )],
        "promise-async-executor",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- parseint-no-radix ---

#[test]
fn single_argument_parseint_is_flagged() {
    let f = rule_findings(
        &[(
            "v.ts",
            "export function toNum(s: string) {\n  return parseInt(s);\n}\n",
        )],
        "parseint-no-radix",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn number_dot_parseint_single_argument_is_flagged() {
    let f = rule_findings(
        &[(
            "v.ts",
            "export function toNum(s: string) {\n  return Number.parseInt(s);\n}\n",
        )],
        "parseint-no-radix",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn parseint_with_explicit_radix_is_not_flagged() {
    let f = rule_findings(
        &[(
            "v.ts",
            "export function toNum(s: string) {\n  return parseInt(s, 10);\n}\n",
        )],
        "parseint-no-radix",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn parseint_wrapping_a_nested_call_is_a_documented_miss_not_flagged() {
    // Documented limitation (never-guess): the single-argument span `[^,()]+` cannot cross a nested call's
    // own parentheses, so `parseInt(getVal())` is silently not flagged rather than guessed at.
    let f = rule_findings(
        &[(
            "v.ts",
            "declare function getVal(): string;\nexport function toNum() {\n  return parseInt(getVal());\n}\n",
        )],
        "parseint-no-radix",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn parseint_radix_ok_marker_suppresses_the_finding() {
    let f = rule_findings(
        &[(
            "v.ts",
            "export function toNum(s: string) {\n  return parseInt(s); // parseint-radix-ok: always base-10 caller-controlled input\n}\n",
        )],
        "parseint-no-radix",
    );
    assert!(f.is_empty(), "{f:?}");
}
