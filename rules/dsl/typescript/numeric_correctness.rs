//! `float-equality` + `always-false-comparison` + `numeric-string-comparison` tests (split from `typescript.rs`).

use super::*;

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
