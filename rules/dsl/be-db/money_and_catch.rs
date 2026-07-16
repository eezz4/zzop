//! `float-money-compare` + `empty-catch-on-write` tests (split from `be-db.rs`).

use super::*;

// --- float-money-compare ---

#[test]
fn strict_equality_on_money_named_identifier_against_a_float_literal_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "export function isBasicPlan(price: number) {\n  return price === 19.99;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "float-money-compare");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn float_literal_first_strict_equality_against_a_money_named_identifier_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "export function isBasicPlan(price: number) {\n  return 19.99 === price;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "float-money-compare");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn strict_equality_between_two_money_named_identifiers_is_not_flagged() {
    // Under-detection by design: a variable-vs-variable comparison is out of reach for a line-scan heuristic.
    // A bare `total` keyword would also substring-match unrelated identifiers like `totalCredits`, so it's excluded.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "export function hasNoCredits(totalCredits: number) {\n  return totalCredits === 0;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "float-money-compare").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn epsilon_based_money_comparison_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const expectedTotal: number;\ndeclare const EPSILON: number;\nexport function isFullyPaid(totalPrice: number) {\n  return Math.abs(totalPrice - expectedTotal) < EPSILON;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "float-money-compare").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn money_ok_marker_directly_above_the_comparison_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "export function isBasicPlanMarked(price: number) {\n  // money-ok: price is stored as integer cents already scaled, exact by construction\n  return price === 19.99;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "float-money-compare").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- empty-catch-on-write ---

#[test]
fn empty_catch_around_a_write_call_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function archiveQuietly(id: string) {\n  try {\n    await prisma.order.update({ where: { id }, data: { archived: true } });\n  } catch (e) {}\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "empty-catch-on-write");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

#[test]
fn catch_that_logs_the_error_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const logger: any;\nexport async function archiveLogged(id: string) {\n  try {\n    await prisma.order.update({ where: { id }, data: { archived: true } });\n  } catch (e) {\n    logger.error(e);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "empty-catch-on-write").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn empty_catch_ok_marker_directly_above_the_catch_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function archiveQuietlyMarked(id: string) {\n  try {\n    await prisma.order.update({ where: { id }, data: { archived: true } });\n  // empty-catch-ok: best-effort archive, failure intentionally ignored\n  } catch (e) {}\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "empty-catch-on-write").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn minified_bundle_with_a_giant_single_line_is_not_flagged() {
    // A bundled/minified `.mjs` file collapses onto a few giant physical lines; `MethodScan`'s line-based
    // span extraction then makes every symbol on such a line spuriously "co-occur" with unrelated
    // write/catch patterns elsewhere on the same line. The engine skips the whole file for every DSL rule
    // pack before any rule runs (`zzop_core::dsl::is_minified_or_generated`). This fixture trips the
    // classifier's RATIO prong: a single ~690-byte line makes up ~96% of the file's bytes (500+ char lines
    // must dominate >= 50% of the file for the file to classify as minified).
    let dir = TempDir::new("zzop-be-db");
    let content = format!(
        "declare const prisma: any;\nconst bundled = \"{}\"; function f() {{ try {{ prisma.order.update({{}}); }} catch (e) {{}} }}\n",
        "x".repeat(600)
    );
    dir.write("src/seed-project/bundle/index.mjs", &content);
    let out = scan(&dir);
    assert!(
        hits(&out, "empty-catch-on-write").is_empty(),
        "{:?}",
        out.findings
    );
}
