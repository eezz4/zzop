use crate::{hits, scan, TempDir};

// --- count-in-loop ---

#[test]
fn store_count_inside_for_of_loop_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "handler.ts",
        "declare const postLikeStore: any;\ndeclare const posts: any[];\nexport async function f() {\n  for (const p of posts) {\n    const c = await postLikeStore.count((l: any) => l.postId === p.id);\n    console.log(c);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "count-in-loop").len(), 1, "{:?}", out.findings);
}

#[test]
fn store_count_inside_map_callback_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "list.ts",
        "declare const postLikeStore: any;\ndeclare const posts: any[];\nexport async function f() {\n  return Promise.all(posts.map(async (p) => ({ id: p.id, c: await postLikeStore.count((l: any) => l.postId === p.id) })));\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "count-in-loop").len(), 1, "{:?}", out.findings);
}

#[test]
fn prisma_model_count_inside_loop_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "prisma.ts",
        "declare const prisma: any;\ndeclare const ids: string[];\nexport async function f() {\n  for (const id of ids) {\n    const c = await prisma.postLike.count({ where: { postId: id } });\n    console.log(c);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "count-in-loop").len(), 1, "{:?}", out.findings);
}

#[test]
fn store_count_outside_loop_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "ok.ts",
        "declare const postLikeStore: any;\nexport async function f(postId: string) {\n  return postLikeStore.count((l: any) => l.postId === postId);\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "count-in-loop").is_empty());
}

/// `.count()` called once, after a `findMany()`, with no loop anywhere in the function — same
/// no-loop-spans-at-all shape as `store_count_outside_loop_is_not_flagged` above, but exercising the
/// findMany-then-single-count adapter pattern specifically.
#[test]
fn count_call_outside_loop_after_findmany_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "summary.ts",
        "declare const postLikeStore: any;\nexport async function f() {\n  const rows = await postLikeStore.findMany();\n  const total = await postLikeStore.count();\n  return { rows, total };\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "count-in-loop").is_empty(), "{:?}", out.findings);
}

// --- app-side-aggregation ---

#[test]
fn findmany_result_reduced_in_app_is_flagged() {
    let dir = TempDir::new("zzop-sql");
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
    let dir = TempDir::new("zzop-sql");
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
    let dir = TempDir::new("zzop-sql");
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
    let dir = TempDir::new("zzop-sql");
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
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "ok.ts",
        "export async function total(store: any) {\n  const agg = await store.aggregate({ _sum: { amount: true } });\n  return agg._sum.amount;\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "app-side-aggregation-reduce").is_empty());
    assert!(hits(&out, "app-side-aggregation-filter-length").is_empty());
}
