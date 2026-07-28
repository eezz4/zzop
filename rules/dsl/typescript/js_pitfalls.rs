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
            "export function delta(total) {\n  return total.toFixed(2) - 1; // zzop-tofixed-arithmetic-ok: re-quantized intentionally\n}\n",
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
fn day_ms_added_to_a_non_epoch_date_value_is_flagged() {
    // Positive pin for the narrowed `day-ms-arithmetic` alternative: the operand is a plain
    // identifier (no call result, so no epoch-ms evidence), shifted by exactly 24h inside
    // `new Date(...)` — a DST-unsafe calendar shift, and a `Date`-to-string coercion bug outright
    // when the identifier holds a `Date`.
    let f = rule_findings(
        &[(
            "v.ts",
            "export function tomorrow(startOfLocalDay: Date) {\n  return new Date(startOfLocalDay + 86400000);\n}\n",
        )],
        "date-pitfalls",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

#[test]
fn day_ms_added_to_a_gettime_call_is_not_flagged() {
    // Calibration pin (6/6 corpus FPs before the fix, this being class #1): `.getTime()` yields epoch
    // milliseconds, and epoch arithmetic is DST-safe BY CONSTRUCTION. This exact fixture used to be
    // this rule's positive pin; the measurement says it was never a defect. The narrowed pattern's
    // operand class (`[^)]*`) cannot cross the call's closing paren, which is what vetoes it.
    let f = rule_findings(
        &[(
            "v.ts",
            "export function tomorrow(d: Date) {\n  return new Date(d.getTime() + 86400000);\n}\n",
        )],
        "date-pitfalls",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn unary_coerced_epoch_operand_still_fires_as_the_documented_residual() {
    // Honest residual pin, asserted by the rule message: `+d` is epoch ms exactly as `.getTime()` is,
    // but it carries no closing paren, so the structural veto cannot see it. Pinned so the message's
    // claim stays true and a future operand-aware matcher flips it deliberately.
    let f = rule_findings(
        &[(
            "v.ts",
            "export function tomorrow(d: Date) {\n  return new Date(+d + 86400000);\n}\n",
        )],
        "date-pitfalls",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn mirrored_operand_order_is_not_flagged() {
    // Scope pin for the message's "the mirrored operand order is out of scope" claim: the day
    // constant must be the RIGHT-hand operand.
    let f = rule_findings(
        &[(
            "v.ts",
            "export function tomorrow(d: number) {\n  return new Date(86400000 + d);\n}\n",
        )],
        "date-pitfalls",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn parenthesized_subexpression_before_the_day_ms_operator_is_not_flagged() {
    // Scope pin for the message's "no closing paren may appear between `new Date(` and the operator"
    // claim — the veto is structural, so a parenthesized subexpression vetoes just like a call does.
    let f = rule_findings(
        &[(
            "v.ts",
            "export function shift(a: number, b: number) {\n  return new Date((a + b) + 86400000);\n}\n",
        )],
        "date-pitfalls",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn epoch_now_day_ms_subtraction_is_not_flagged() {
    // FP class #2: a retention cutoff computed in epoch space. No `new Date(` head at all.
    let f = rule_findings(
        &[(
            "v.ts",
            "export const cutoff = Date.now() - 30 * 86400000;\n",
        )],
        "date-pitfalls",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn gettime_difference_divided_by_day_ms_is_not_flagged() {
    // FP class #3: a day COUNT between two instants — division by the day constant, not a shift.
    let f = rule_findings(
        &[(
            "v.ts",
            "export function daysBetween(a: Date, b: Date) {\n  return (a.getTime() - b.getTime()) / 86400000;\n}\n",
        )],
        "date-pitfalls",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn date_utc_anchored_day_ms_arithmetic_is_not_flagged() {
    // FP class #4: ISO-week math anchored on `Date.UTC(...)` — literally the UTC-only construction
    // this rule's own message recommends as the fix, so flagging it was self-contradictory.
    let f = rule_findings(
        &[(
            "v.ts",
            "export function isoWeekStart(y: number, m: number, d: number) {\n  return new Date(Date.UTC(y, m, d) + 86400000);\n}\n",
        )],
        "date-pitfalls",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn julian_day_to_epoch_ms_conversion_is_not_flagged() {
    // FP class #5: a days-to-milliseconds unit CONVERSION (`* 86400000`). The day constant is a
    // multiplicative factor here, never an additive shift of a date value.
    let f = rule_findings(
        &[(
            "v.ts",
            "export function fromJulian(jd: number) {\n  return new Date((jd - 2440587.5) * 86400000);\n}\n",
        )],
        "date-pitfalls",
    );
    assert!(f.is_empty(), "{f:?}");
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
            "// zzop-date-pitfalls-ok: server is UTC-only, epoch is confirmed seconds\nexport const d = new Date(1700000000);\n",
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
    // Sibling boundary: `.map(async ...)` is a different defect owned by reliability/map-async-no-promise-all.
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
            "export async function run(items) {\n  items.forEach(async (item) => { // zzop-foreach-async-callback-ok: fire-and-forget by design\n    await save(item);\n  });\n}\n",
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
            "export function wrap() {\n  // zzop-promise-async-executor-ok: rejections are handled by the caller's catch\n  return new Promise(async (resolve) => { resolve(await load()); });\n}\n",
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
            "export function toNum(s: string) {\n  return parseInt(s); // zzop-parseint-no-radix-ok: always base-10 caller-controlled input\n}\n",
        )],
        "parseint-no-radix",
    );
    assert!(f.is_empty(), "{f:?}");
}
