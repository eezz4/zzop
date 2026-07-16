//! `tofixed-arithmetic` + `date-pitfalls` + `foreach-async-callback` + `promise-async-executor` + `parseint-no-radix` tests (split from `typescript.rs`).

use super::*;

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
