//! Exercises `rules/dsl/sql/sql.json`'s SQL/ORM-usage rule pack end-to-end via `zzop_engine::analyze_tree` so
//! `Matcher::MethodScan` rules run against real parser-derived `SourceSymbol` body spans. See `sql.json` for
//! each rule's exact matcher shape and message.
//!
//! `query-logic-density` counts CASE-WHEN branches within one SQL literal via a whole-file `require_file`
//! gate (an SQL anchor keyword plus two `WHEN`s) paired with a `line_pattern` on the literal's `CASE` line,
//! since `Matcher::LineScan` has no cross-line aggregation.
//!
//! `app-side-aggregation-reduce`/`-filter-length` and `race-condition-toctou` are co-occurrence
//! approximations: method-scan has no variable-binding memory, so they don't verify the same variable is
//! on both sides of the pattern (a guard/receiver anywhere in the function body counts).
//!
//! Out of scope (a check that can't be expressed accurately ships as nothing, not half-right):
//! cache-invalidation-on-write (needs cross-file key-vocabulary resolution) and hardcoded-record-ref
//! detection (needs AST-structural object-literal traversal) — both beyond the DSL's four matcher shapes.
//!
//! Every rule's `// <marker>-ok:` suppression case is covered below, using the fixed "finding's own line
//! OR the single line directly above" window (`MARKER_LOOKBACK_LINES`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RulePackDef;
use zzop_engine::{analyze_tree, AnalyzeOutput, DispatchConfig, EngineConfig, DEFAULT_SIZE_CAP};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Loads the real `sql.json` pack, co-located with this test file.
fn sql_pack() -> RulePackDef {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl/sql/sql.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("parse sql.json")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "sql-fixture".to_string(),
        dispatch: DispatchConfig::default(),
        size_cap: DEFAULT_SIZE_CAP,
        rule_config: Default::default(),
        packs: vec![sql_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("sql/{rule}"))
        .collect()
}

// --- query-logic-density ---

#[test]
fn counts_case_when_branches_in_multiline_sql_template_over_threshold() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "report.ts",
        "export const q = `\n  SELECT id,\n    CASE\n      WHEN tier = 'gold' THEN price * 0.8\n      WHEN tier = 'silver' THEN price * 0.9\n      ELSE price\n    END AS final\n  FROM orders WHERE active = true\n`;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "query-logic-density");
    assert_eq!(h.len(), 1, "expected 1 hit, got: {:?}", out.findings);
    assert_eq!(h[0].file, "report.ts");
}

#[test]
fn does_not_count_aggregation_only_sql() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "agg.ts",
        "export const q = `\n  SELECT customer_id, SUM(amount) AS total, COUNT(*) AS n\n  FROM orders GROUP BY customer_id\n`;\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "query-logic-density").is_empty());
}

#[test]
fn single_case_when_is_below_threshold() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "one.ts",
        "export const q = `SELECT CASE WHEN active THEN 1 ELSE 0 END AS flag FROM users`;\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "query-logic-density").is_empty());
}

#[test]
fn ignores_ordinary_js_switch_case() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "code.ts",
        "export function f(x: number) {\n  if (x > 0) return 1;\n  switch (x) { case 1: return 2; default: return 3; }\n  return 0;\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "query-logic-density").is_empty());
}

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

// --- race-condition-toctou (uses `absent` labels) ---

#[test]
fn toggle_pattern_findone_then_delete_else_create_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createPostHandlers.ts",
        "declare const postLikeStore: any;\nexport async function toggleLike() {\n  const existing = await postLikeStore.findOne((l: any) => l.id === \"x\");\n  if (existing) {\n    await postLikeStore.delete(existing.id);\n  } else {\n    await postLikeStore.create({ id: \"y\" });\n  }\n}\n",
    );
    let out = scan(&dir);
    let hits = hits(&out, "race-condition-toctou");
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    // The finding's line is the READ declaration's line (3), not the write call's line (7) — `trigger` is
    // `read`, so the reported line marks where the race window opens.
    assert_eq!(hits[0].line, 3, "{:?}", out.findings);
}

#[test]
fn findone_plus_if_create_only_no_else_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createSubHandlers.ts",
        "declare const subStore: any;\nexport async function subscribe() {\n  const existing = await subStore.findOne((s: any) => s.id === \"x\");\n  if (!existing) {\n    await subStore.create({ id: \"y\" });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "race-condition-toctou").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn toggle_guarded_by_try_catch_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createGuardedHandlers.ts",
        "declare const likeStore: any;\nexport async function toggle() {\n  const existing = await likeStore.findOne((l: any) => l.id === \"x\");\n  if (existing) {\n    await likeStore.delete(existing.id);\n  } else {\n    try {\n      await likeStore.create({ id: \"y\" });\n    } catch (e) {\n      // P2002 idempotent\n    }\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "race-condition-toctou").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn read_only_no_write_operations_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createReadOnly.ts",
        "declare const itemStore: any;\nexport async function get() {\n  const existing = await itemStore.findOne((s: any) => s.id === \"x\");\n  if (!existing) return null;\n  return existing;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "race-condition-toctou").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn nested_prisma_model_receiver_toggle_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createLikeHandlers.ts",
        "declare const prisma: any;\nexport async function toggle() {\n  const existing = await prisma.like.findUnique({ where: { id: \"x\" } });\n  if (existing) {\n    await prisma.like.delete({ where: { id: existing.id } });\n  } else {\n    await prisma.like.create({ data: { id: \"y\" } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "race-condition-toctou").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn transaction_wrapped_toggle_is_still_flagged() {
    // A bare $transaction does NOT close a check-then-act race at READ COMMITTED — two concurrent
    // transactions can both read empty and both insert. The old `tx-guard` veto encoded the wrong
    // fix (matching the be-db sibling `find-then-create-no-unique` correction), so this fixture,
    // previously pinned as a negative, is now a positive.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createTxHandlers.ts",
        "declare const prisma: any;\nexport async function toggle() {\n  await prisma.$transaction(async () => {\n    const existing = await prisma.like.findUnique({ where: { id: \"x\" } });\n    if (!existing) {\n      await prisma.like.create({ data: { id: \"y\" } });\n    }\n  });\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "race-condition-toctou").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn select_for_update_locked_toggle_is_not_flagged() {
    // SELECT ... FOR UPDATE is one of the message's recommended atomic escapes — the row lock
    // serializes the concurrent readers, so the check-then-act shape is safe and stays silent.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createLockHandlers.ts",
        "declare const db: any;\nexport async function toggle() {\n  const existing = await db.findOne(\"SELECT * FROM likes WHERE id = $1 FOR UPDATE\");\n  if (!existing) {\n    await db.insert(\"likes\", { id: \"y\" });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "race-condition-toctou").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn toctou_ok_marker_directly_above_the_read_line_suppresses_the_finding() {
    // The marker sits directly above the READ declaration line; since `trigger` is `read`, that IS the reported line.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createMarkedHandlers.ts",
        "declare const likeStore: any;\nexport async function toggle() {\n  // toctou-ok: intentional single-writer admin path\n  const existing = await likeStore.findOne((l: any) => l.id === \"x\");\n  if (existing) {\n    await likeStore.delete(existing.id);\n  } else {\n    await likeStore.create({ id: \"y\" });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "race-condition-toctou").is_empty(),
        "{:?}",
        out.findings
    );
}

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

// --- TypeScript `switch/case` clauses must never satisfy query-logic-density: a bare `\bcase\b` line
// pattern would fire on `case 'sum':` labels whenever the file also contains incidental "when"/"from"/"set"
// words, so the SQL-shaped pattern requires `CASE WHEN ...` or a bare line-ending `CASE`. ---

#[test]
fn typescript_switch_cases_do_not_fire_query_logic_density() {
    let dir = TempDir::new("zzop-sql");
    // Gate bait on purpose: "when"/"from"/"values" appear as ordinary prose/identifiers.
    dir.write(
        "aggregate.ts",
        "// chooses the aggregator when values come from the grid; picks when needed\nexport function createAggregator(type: string) {\n  switch (type) {\n    case 'sum':\n      return 1;\n    case 'count':\n      return 2;\n    default:\n      return 0;\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "sql/query-logic-density"),
        "{:?}",
        out.findings
    );
}

/// The SQL shapes still fire: single-line `CASE WHEN` and the multiline bare `CASE` both anchor.
#[test]
fn single_line_case_when_still_fires_query_logic_density() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "pricing.ts",
        "export const q = `SELECT id, CASE WHEN tier = 'gold' THEN 1 WHEN tier = 'silver' THEN 2 ELSE 0 END FROM orders`;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "query-logic-density");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
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
