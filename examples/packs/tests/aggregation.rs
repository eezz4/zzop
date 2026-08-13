//! Exercises `examples/packs/sql-preferences.json`'s `app-side-aggregation-reduce` and
//! `app-side-aggregation-filter-length` method-scan rules.
//!
//! Split out of `rules/dsl/sql/aggregation.rs`, which judged these two AND the bundled
//! `sql/count-in-loop` from one file. The count-in-loop half stayed there; these five moved with the
//! rules, unchanged apart from the pack the helper loads. Both rules are co-occurrence approximations:
//! method-scan has no variable-binding memory, so they do not verify the same variable is on both
//! sides of the pattern.

use crate::{hits, scan, TempDir};

// --- app-side-aggregation ---

#[test]
fn findmany_result_reduced_in_app_is_flagged() {
    let dir = TempDir::new("zzop-sql-preferences");
    dir.write(
        "sum.ts",
        "export async function total(store: any) {\n  const rows = await store.findMany({ where: { active: true } });\n  return rows.reduce((s: number, r: any) => s + r.amount, 0);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "app-side-aggregation-reduce").len(),
        1,
        "{:?}",
        out.findings
    );
    assert!(hits(&out, "app-side-aggregation-filter-length").is_empty());
}

#[test]
fn findmany_result_counted_via_filter_length_is_flagged() {
    let dir = TempDir::new("zzop-sql-preferences");
    dir.write(
        "count.ts",
        "export async function activeCount(store: any) {\n  const items = await store.findMany();\n  return items.filter((r: any) => r.active).length;\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "app-side-aggregation-filter-length").len(),
        1,
        "{:?}",
        out.findings
    );
    assert!(hits(&out, "app-side-aggregation-reduce").is_empty());
}

#[test]
fn raw_d1_prepare_all_reduced_in_app_is_flagged() {
    let dir = TempDir::new("zzop-sql-preferences");
    dir.write(
        "d1.ts",
        "export async function total(env: any) {\n  const rows = await env.DB.prepare(\"SELECT amount FROM orders\").all();\n  return rows.reduce((s: number, r: any) => s + r.amount, 0);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "app-side-aggregation-reduce").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn aggregation_on_unrelated_variable_is_not_flagged() {
    let dir = TempDir::new("zzop-sql-preferences");
    dir.write(
        "x.ts",
        "export function f(nums: number[]) { return nums.reduce((a, b) => a + b, 0); }\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "app-side-aggregation-reduce").is_empty());
    assert!(hits(&out, "app-side-aggregation-filter-length").is_empty());
}

#[test]
fn sql_aggregate_done_in_db_is_not_flagged() {
    let dir = TempDir::new("zzop-sql-preferences");
    dir.write(
        "ok.ts",
        "export async function total(store: any) {\n  const agg = await store.aggregate({ _sum: { amount: true } });\n  return agg._sum.amount;\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "app-side-aggregation-reduce").is_empty());
    assert!(hits(&out, "app-side-aggregation-filter-length").is_empty());
}
