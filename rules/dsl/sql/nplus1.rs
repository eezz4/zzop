use crate::{hits, scan, TempDir};

// --- nplus1 ---

#[test]
fn await_store_findone_in_for_of_loop_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "domains/sample/routes/createSampleHandlers.ts",
        "declare const sampleStore: any;\ndeclare const memberIds: string[];\nexport async function f() {\n  for (const id of memberIds) {\n    const g = await sampleStore.findOne((x: any) => x.id === id);\n    console.log(g);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "nplus1").len(), 1, "{:?}", out.findings);
}

#[test]
fn await_prisma_findfirst_in_for_in_loop_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "domains/user/routes/createUserHandlers.ts",
        "declare const prisma: any;\ndeclare const idMap: Record<string, boolean>;\nexport async function f() {\n  for (const id in idMap) {\n    const u = await prisma.findFirst({ where: { id } });\n    console.log(u);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "nplus1").len(), 1, "{:?}", out.findings);
}

#[test]
fn await_store_findbyid_in_traditional_for_loop_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createOrderHandlers.ts",
        "declare const orderStore: any;\ndeclare const ids: string[];\nexport async function f() {\n  for (let i = 0; i < ids.length; i++) {\n    const o = await orderStore.findById(ids[i]);\n    console.log(o);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "nplus1").len(), 1, "{:?}", out.findings);
}

#[test]
fn await_store_findmany_in_map_async_callback_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "domains/post/routes/createPostHandlers.ts",
        "declare const postStore: any;\ndeclare const userIds: string[];\nexport async function f() {\n  return userIds.map(async (uid) => {\n    return await postStore.findMany({ userId: uid });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "nplus1").len(), 1, "{:?}", out.findings);
}

#[test]
fn await_store_delete_in_foreach_callback_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createCleanupHandlers.ts",
        "declare const sessionStore: any;\ndeclare const sessions: any[];\nexport async function f() {\n  sessions.forEach(async (s) => {\n    await sessionStore.delete(s.id);\n  });\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "nplus1").len(), 1, "{:?}", out.findings);
}

#[test]
fn await_store_inside_promise_all_map_is_still_flagged() {
    // A store call inside `Promise.all(...map(...))` is still flagged: this rule detects presence only, not parallel-execution intent.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "domains/sample/routes/createSampleHandlers.ts",
        "declare const sampleStore: any;\ndeclare const ids: string[];\nexport async function f() {\n  const results = await Promise.all(\n    ids.map(async (id) => {\n      return await sampleStore.findOne((x: any) => x.id === id);\n    })\n  );\n  return results;\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "nplus1").len(), 1, "{:?}", out.findings);
}

#[test]
fn single_await_store_outside_loop_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "domains/order/routes/createOrderHandlers.ts",
        "declare const orderStore: any;\nexport async function getOrder(orderId: string) {\n  const order = await orderStore.findOne((o: any) => o.id === orderId);\n  return order;\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "nplus1").is_empty());
}

#[test]
fn prisma_findmany_called_once_outside_loop_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createBulkHandlers.ts",
        "declare const prisma: any;\ndeclare const userIds: string[];\nexport async function f() {\n  const posts = await prisma.post.findMany({ where: { userId: { in: userIds } } });\n  return posts;\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "nplus1").is_empty());
}

#[test]
fn file_outside_domains_or_api_paths_is_excluded() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "lib/helpers.ts",
        "declare const userStore: any;\ndeclare const ids: string[];\nexport async function f() {\n  for (const id of ids) {\n    await userStore.findOne((u: any) => u.id === id);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "nplus1").is_empty());
}

// --- structural span-based containment: trigger_in_loop (rewritten from text co-occurrence) ---

#[test]
fn await_store_finduniq_inside_for_of_loop_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "domains/user/routes/createUserHandlers.ts",
        "declare const userStore: any;\ndeclare const users: any[];\nexport async function f() {\n  for (const u of users) {\n    await userStore.findUnique({ where: { id: u.id } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "nplus1").len(), 1, "{:?}", out.findings);
}

/// Adapter shape (mirrors `perf/api-in-loop`'s REDDIT-shape negative): one `findMany`, then the result
/// array is TRANSFORMED via `.map()` — the `await ... findMany(` line is not textually inside the map
/// callback's own span, so the trigger never satisfies inside a loop span and the rule stays silent.
#[test]
fn findmany_then_result_array_map_transform_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/listUsers.ts",
        "declare const userStore: any;\nexport async function f() {\n  const results = await userStore.findMany({ where: { active: true } });\n  return results.map((u: any) => ({ id: u.id, name: u.name }));\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "nplus1").is_empty(), "{:?}", out.findings);
}
