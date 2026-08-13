//! The `// zzop-<rule>-ok` marker cases for this pack's three method-scan/line-scan rules that have
//! one, moved out of `rules/dsl/sql/suppression.rs` when the rules left the bundle. That file kept the
//! markers of the rules that stayed (`nplus1`, `count-in-loop`, `race-condition-toctou`).
//!
//! ⚠ These are the load-bearing cases for the export's central compatibility claim: the marker is
//! derived from the BARE rule id (`zzop_core`'s `suppress_marker_for_id`), so `// zzop-select-star-ok`
//! sitting in a user's tree keeps suppressing after the rule's qualified id changed from
//! `sql/select-star` to `sql-preferences/select-star`. If the derivation ever became
//! pack-qualified, these tests are where it fails.

use crate::{hits, scan, TempDir};

#[test]
fn query_logic_ok_marker_directly_above_the_case_line_suppresses_the_finding() {
    // The marker sits directly above the `CASE` line itself; the marker check has no comment-syntax
    // awareness, so a `//`-prefixed line inside a template literal works identically to a real comment.
    let dir = TempDir::new("zzop-sql-preferences");
    dir.write(
        "ok.ts",
        "export const q = `\n  SELECT id,\n  // zzop-query-logic-density-ok: legacy pricing view, owned by analytics\n  CASE WHEN a THEN 1 WHEN b THEN 2 END FROM t WHERE x\n`;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "query-logic-density").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn app_agg_ok_marker_suppresses_the_reduce_finding() {
    let dir = TempDir::new("zzop-sql-preferences");
    dir.write(
        "ok2.ts",
        "export async function total(store: any) {\n  const rows = await store.findMany();\n  // zzop-app-side-aggregation-reduce-ok: bounded to <=50 rows by upstream guard\n  return rows.reduce((s: number, r: any) => s + r.amount, 0);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "app-side-aggregation-reduce").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn app_agg_filter_ok_marker_suppresses_the_filter_length_finding() {
    // `app-side-aggregation-reduce` and `app-side-aggregation-filter-length` each need their own marker
    // (`zzop-app-side-aggregation-reduce-ok` vs `zzop-app-side-aggregation-filter-length-ok`) so suppressing one can't silently suppress the other.
    let dir = TempDir::new("zzop-sql-preferences");
    dir.write(
        "ok3.ts",
        "export async function count(store: any) {\n  const rows = await store.findMany();\n  // zzop-app-side-aggregation-filter-length-ok: bounded to <=50 rows by upstream guard\n  return rows.filter((r: any) => r.active).length;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "app-side-aggregation-filter-length").is_empty(),
        "{:?}",
        out.findings
    );
}
