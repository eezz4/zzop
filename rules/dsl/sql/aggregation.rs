//! `count-in-loop` only, since 2026-08-12. This file also held the five
//! `app-side-aggregation-{reduce,filter-length}` fixtures until those two rules left the bundle for
//! `examples/packs/sql-preferences.json`; they moved to that pack's own `aggregation.rs` rather than
//! being deleted, so the export cost no coverage.

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
    // MULTILINE callback on purpose: a single-line `.map(...)` callback gets no loop span (the span
    // cannot prove containment line-granularly — `SourceFile::loop_spans`'s doc owns that rule), so
    // the per-iteration count is asserted on a callback with lines of its own.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "list.ts",
        "declare const postLikeStore: any;\ndeclare const posts: any[];\nexport async function f() {\n  return Promise.all(posts.map(async (p) => ({\n    id: p.id,\n    c: await postLikeStore.count((l: any) => l.postId === p.id),\n  })));\n}\n",
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
