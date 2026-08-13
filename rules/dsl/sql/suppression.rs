//! The `// zzop-<rule>-ok` marker cases for the rules this pack still ships. Three more lived here
//! until 2026-08-12 — `query-logic-density` and the two `app-side-aggregation-*` rules — and moved with
//! their rules to `examples/packs/tests/suppression.rs` rather than being deleted.

use crate::{hits, scan, TempDir};

// --- marker-suppression cases ---

#[test]
fn n_plus_1_ok_marker_above_the_store_call_whitelists_the_for_of_loop() {
    // The marker sits directly above the `store-call` trigger line, not the `for` loop line.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "domains/notification/routes/createNotifHandlers.ts",
        "declare const notifStore: any;\ndeclare const users: any[];\nexport async function f() {\n  for (const u of users) {\n    // zzop-nplus1-ok: intentional sequential processing for cascade delete\n    await notifStore.delete(u.id);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "nplus1").is_empty(), "{:?}", out.findings);
}

#[test]
fn n_plus_1_ok_marker_above_the_store_call_whitelists_the_map_callback() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createMigrateHandlers.ts",
        "declare const legacyStore: any;\ndeclare const items: any[];\nexport async function f() {\n  await Promise.all(items.map(async (item) => {\n    // zzop-nplus1-ok: one-time migration job\n    await legacyStore.create(item);\n  }));\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "nplus1").is_empty(), "{:?}", out.findings);
}

#[test]
fn count_in_loop_ok_marker_present_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "marked.ts",
        "declare const postLikeStore: any;\ndeclare const posts: any[];\nexport async function f() {\n  for (const p of posts) {\n    // zzop-count-in-loop-ok: small fixed iteration, intentional sequential\n    const c = await postLikeStore.count((l: any) => l.postId === p.id);\n    console.log(c);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "count-in-loop").is_empty(), "{:?}", out.findings);
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
