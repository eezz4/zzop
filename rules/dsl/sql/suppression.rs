use crate::{hits, scan, TempDir};

// --- marker-suppression cases ---

#[test]
fn query_logic_ok_marker_directly_above_the_case_line_suppresses_the_finding() {
    // The marker sits directly above the `CASE` line itself; the marker check has no comment-syntax
    // awareness, so a `//`-prefixed line inside a template literal works identically to a real comment.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "ok.ts",
        "export const q = `\n  SELECT id,\n  // query-logic-ok: legacy pricing view, owned by analytics\n  CASE WHEN a THEN 1 WHEN b THEN 2 END FROM t WHERE x\n`;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "query-logic-density").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn n_plus_1_ok_marker_above_the_store_call_whitelists_the_for_of_loop() {
    // The marker sits directly above the `store-call` trigger line, not the `for` loop line.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "domains/notification/routes/createNotifHandlers.ts",
        "declare const notifStore: any;\ndeclare const users: any[];\nexport async function f() {\n  for (const u of users) {\n    // n+1-ok: intentional sequential processing for cascade delete\n    await notifStore.delete(u.id);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "nplus1").is_empty(), "{:?}", out.findings);
}

#[test]
fn n_plus_1_ok_marker_above_the_store_call_whitelists_the_map_callback() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createMigrateHandlers.ts",
        "declare const legacyStore: any;\ndeclare const items: any[];\nexport async function f() {\n  await Promise.all(items.map(async (item) => {\n    // n+1-ok: one-time migration job\n    await legacyStore.create(item);\n  }));\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "nplus1").is_empty(), "{:?}", out.findings);
}

#[test]
fn count_in_loop_ok_marker_present_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "marked.ts",
        "declare const postLikeStore: any;\ndeclare const posts: any[];\nexport async function f() {\n  for (const p of posts) {\n    // count-in-loop-ok: small fixed iteration, intentional sequential\n    const c = await postLikeStore.count((l: any) => l.postId === p.id);\n    console.log(c);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "count-in-loop").is_empty(), "{:?}", out.findings);
}

#[test]
fn app_agg_ok_marker_suppresses_the_reduce_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "ok2.ts",
        "export async function total(store: any) {\n  const rows = await store.findMany();\n  // app-agg-ok: bounded to <=50 rows by upstream guard\n  return rows.reduce((s: number, r: any) => s + r.amount, 0);\n}\n",
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
    // (`app-agg-ok` vs `app-agg-filter-ok`) so suppressing one can't silently suppress the other.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "ok3.ts",
        "export async function count(store: any) {\n  const rows = await store.findMany();\n  // app-agg-filter-ok: bounded to <=50 rows by upstream guard\n  return rows.filter((r: any) => r.active).length;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "app-side-aggregation-filter-length").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- skip_comment_lines + test-path file_exclude_pattern ---
// A commented-out read-then-write toggle shape must not fire `race-condition-toctou`, and every rule in
// this pack excludes test-fixture paths.

#[test]
fn toctou_shape_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createPostHandlers.ts",
        "declare const postLikeStore: any;\nexport async function toggleLike() {\n  // const existing = await postLikeStore.findOne(...) -- old racy version, replaced\n  return null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "race-condition-toctou").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn nplus1_loop_in_an_api_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/__tests__/createOrderHandlers.ts",
        "declare const orderStore: any;\ndeclare const ids: string[];\nexport async function f() {\n  for (let i = 0; i < ids.length; i++) {\n    const o = await orderStore.findById(ids[i]);\n    console.log(o);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "nplus1").is_empty(), "{:?}", out.findings);
}
