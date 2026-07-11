//! End-to-end tests for `rules/dsl/be-db/be-db.json`, exercised via `zzop_engine::analyze_tree` so
//! `Matcher::MethodScan` rules run against real parser-derived `SourceSymbol` body spans (not hand-built
//! spans). See `be-db.json` for each rule's exact trigger/veto shape and message.
//!
//! `client-per-request` needs a negative fixture proving a module-top-level singleton is never scanned at
//! all: `MethodScan` only evaluates `SourceFile::symbols` body spans, and a top-level statement has no
//! enclosing function span — see `module_top_level_singleton_prisma_client_is_not_flagged`.
//!
//! Every rule's `suppress_marker` is exercised once below, with the marker directly above the
//! reported/trigger line (`MARKER_LOOKBACK_LINES` = 1 — the only lookback distance that suppresses).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — same pattern as `sql/sql.rs`).
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

/// Loads the real `be-db.json` pack, filtered so this test is unaffected by sibling packs under
/// concurrent development.
fn be_db_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "be-db")
        .expect("be-db pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "be-db-fixture".to_string(),
        packs: vec![be_db_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("be-db/{rule}"))
        .collect()
}

// --- update-delete-no-where ---

#[test]
fn update_many_with_no_where_anywhere_in_function_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function bulkArchive() {\n  await prisma.order.updateMany({ data: { archived: true } });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "update-delete-no-where");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    assert_eq!(h[0].file, "src/service.ts");
}

#[test]
fn delete_many_with_where_in_same_function_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function archiveOld() {\n  await prisma.order.deleteMany({ where: { archived: false } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_many_with_arrow_predicate_first_arg_is_not_flagged() {
    // A custom Store wrapper's `deleteMany(predicate)` takes a filter function scoped internally, not a Prisma-style `{ where: ... }` object — not a whole-table write.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/store.ts",
        "declare const guildShareStore: any;\nexport async function removeSpaceShares(spaceId: string) {\n  await guildShareStore.deleteMany((s: any) => s.spaceId === spaceId);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_many_with_no_arg_predicate_shorthand_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/store.ts",
        "declare const sessionStore: any;\nexport async function clearAllSessions() {\n  await sessionStore.deleteMany(() => true);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_many_with_function_keyword_predicate_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/store.ts",
        "declare const recordStore: any;\nexport async function purgeExpired() {\n  await recordStore.deleteMany(function (r: any) { return r.expired; });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_many_with_no_arguments_is_still_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function wipeOrders() {\n  await prisma.order.deleteMany();\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "update-delete-no-where");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn no_where_ok_marker_directly_above_the_bulk_write_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function bulkArchiveMarked() {\n  // no-where-ok: admin console confirmed intentional full-table archive\n  await prisma.order.updateMany({ data: { archived: true } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- pagination-no-orderby ---

#[test]
fn skip_take_pagination_with_no_orderby_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function listUsers(page: number) {\n  return prisma.user.findMany({ skip: page * 20, take: 20 });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "pagination-no-orderby");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn skip_take_pagination_with_orderby_in_same_function_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function listUsersSorted(page: number) {\n  return prisma.user.findMany({ skip: page * 20, take: 20, orderBy: { id: \"asc\" } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "pagination-no-orderby").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn comment_mentioning_the_word_skip_with_a_colon_is_not_flagged() {
    // A comment documenting a `skip:` parameter can satisfy the `pagination` pattern's `\bskip\s*:` shape even with no real pagination call in the function.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "export async function listUsersDocumented(page: number) {\n  // Note: skip: this function currently loads everything, pagination not yet implemented\n  return [];\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "pagination-no-orderby").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pagination_ok_marker_directly_above_the_pagination_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function listUsersMarked(page: number) {\n  // pagination-ok: single-admin dashboard, deterministic dataset snapshot\n  return prisma.user.findMany({ skip: page * 20, take: 20 });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "pagination-no-orderby").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn apollo_skip_option_with_no_find_many_in_function_is_not_flagged() {
    // Apollo Client's `useQuery(..., { skip: boolean })` flag shares the `skip:` option-name shape with
    // Prisma's row-offset pagination option but has nothing to do with a database; requiring a `.findMany(` co-occurrence excludes it.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/hooks/usePageLayout.tsx",
        "declare function useQuery(query: unknown, options: { skip: boolean }): { data: unknown };\ndeclare function isDefined(v: unknown): boolean;\ndeclare const pageLayoutQuery: unknown;\ndeclare const isOnPageLayoutPage: boolean;\ndeclare const pageLayoutId: string | undefined;\nexport function usePageLayout() {\n  return useQuery(pageLayoutQuery, { skip: !isOnPageLayoutPage || !isDefined(pageLayoutId) });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "pagination-no-orderby").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- client-per-request ---

#[test]
fn new_prisma_client_inside_request_handler_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/handler.ts",
        "declare class PrismaClient { user: any; }\nexport async function handleRequest(req: any, res: any) {\n  const prisma = new PrismaClient();\n  const users = await prisma.user.findMany();\n  res.json(users);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "client-per-request");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn module_top_level_singleton_prisma_client_is_not_flagged() {
    // A module-top-level `new PrismaClient()` has no enclosing function span, so it's never scanned here regardless of the handler-context co-pattern (see module doc).
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/handler.ts",
        "declare class PrismaClient { user: any; }\nconst prisma = new PrismaClient();\nexport async function handleRequest(req: any, res: any) {\n  const users = await prisma.user.findMany();\n  res.json(users);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-per-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn prisma_client_ok_marker_directly_above_the_new_client_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/handler.ts",
        "declare class PrismaClient { user: any; }\nexport async function handleAdminRequest(req: any, res: any) {\n  // prisma-client-ok: cold-start admin tool, single-invocation script\n  const prisma = new PrismaClient();\n  const users = await prisma.user.findMany();\n  res.json(users);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-per-request").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- external-call-in-tx ---

#[test]
fn fetch_call_inside_transaction_callback_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function fetch(url: string, init?: any): Promise<any>;\nexport async function checkoutOrder(orderId: string) {\n  await prisma.$transaction(async (tx: any) => {\n    await tx.order.update({ where: { id: orderId }, data: { status: \"paid\" } });\n    await fetch(\"https://payments.example.com/notify\", { method: \"POST\" });\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "external-call-in-tx");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

#[test]
fn fetch_call_with_no_transaction_in_function_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function fetch(url: string, init?: any): Promise<any>;\nexport async function checkoutOrderSafe(orderId: string) {\n  const paymentResult = await fetch(\"https://payments.example.com/notify\", { method: \"POST\" });\n  await prisma.order.update({ where: { id: orderId }, data: { status: \"paid\" } });\n  return paymentResult;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "external-call-in-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_egress_ok_marker_directly_above_the_fetch_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function fetch(url: string, init?: any): Promise<any>;\nexport async function checkoutOrderMarked(orderId: string) {\n  await prisma.$transaction(async (tx: any) => {\n    await tx.order.update({ where: { id: orderId }, data: { status: \"paid\" } });\n    // tx-egress-ok: payment gateway called via idempotent webhook retry, safe inside tx\n    await fetch(\"https://payments.example.com/notify\", { method: \"POST\" });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "external-call-in-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- unawaited-write ---

#[test]
fn fire_and_forget_create_call_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function logEvent(id: string) {\n  prisma.event.create({ data: { id } });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-write");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    assert_eq!(h[0].file, "src/service.ts");
}

#[test]
fn captured_promise_create_call_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function logEventCaptured(id: string) {\n  const p = prisma.event.create({ data: { id } });\n  return p;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-write").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn unawaited_ok_marker_directly_above_the_write_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function logEventMarked(id: string) {\n  // unawaited-ok: best-effort audit log, failure intentionally ignored\n  prisma.event.create({ data: { id } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-write").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn in_memory_set_delete_is_not_flagged() {
    // The receiver allowlist excludes non-DB calls like an in-memory Set/Map `.delete()`/`.create()`.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/tabs.ts",
        "declare const attachedTabs: Set<string>;\nexport function detachTab(id: string) {\n  attachedTabs.delete(id);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-write").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn in_memory_map_delete_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/cache.ts",
        "declare const cache: Map<string, unknown>;\nexport function evict(k: string) {\n  cache.delete(k);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-write").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn report_update_is_not_flagged() {
    // `report` starts with `repo`, so a naive `repo\w*` receiver group would over-match a non-DB `report.update(...)` call.
    // The receiver group `repo(sitory|sitories)?s?` matches only `repo`/`repos`/`repository`/`repositories`.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/report.ts",
        "declare const report: { update: (data: unknown) => void };\nexport function refreshReport(data: unknown) {\n  report.update(data);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-write").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn fire_and_forget_prisma_user_create_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function trackSignup(email: string) {\n  prisma.user.create({ data: { email } });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-write");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn awaited_prisma_user_create_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function trackSignupAwaited(email: string) {\n  await prisma.user.create({ data: { email } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-write").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- unbounded-user-limit ---

#[test]
fn take_sourced_directly_from_req_query_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const req: any;\nexport async function listUsers() {\n  return prisma.user.findMany({ take: Number(req.query.limit) });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "unbounded-user-limit");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn take_clamped_with_math_min_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const req: any;\nexport async function listUsersClamped() {\n  return prisma.user.findMany({ take: Math.min(50, Number(req.query.limit)) });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unbounded-user-limit").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn limit_ok_marker_directly_above_the_take_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const req: any;\nexport async function listUsersMarked() {\n  // limit-ok: internal admin tool, request volume trusted\n  return prisma.user.findMany({ take: Number(req.query.limit) });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unbounded-user-limit").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- find-then-create-no-unique ---

#[test]
fn find_first_then_create_with_no_unique_guard_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function ensureUser(email: string) {\n  const existing = await prisma.user.findFirst({ where: { email } });\n  if (!existing) {\n    await prisma.user.create({ data: { email } });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "find-then-create-no-unique");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

#[test]
fn find_first_then_create_wrapped_only_in_transaction_is_still_flagged() {
    // A bare `$transaction(...)` wrap is no longer treated as a fix: at the database's default READ
    // COMMITTED isolation level, two concurrent transactions can both read empty and both insert, so this
    // still races and must fire. (Previously this was the pack's negative/veto fixture for a `$transaction`
    // wrap; the veto was removed because it no longer reflects a real fix — see `find-then-create-no-unique`'s
    // message.)
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function ensureUserStillRacy(email: string) {\n  await prisma.$transaction(async (tx: any) => {\n    const existing = await tx.user.findFirst({ where: { email } });\n    if (!existing) {\n      await tx.user.create({ data: { email } });\n    }\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "find-then-create-no-unique");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn find_first_then_create_alongside_a_unique_constraint_backed_upsert_is_not_flagged() {
    // Veto-mechanism test (same co-occurrence-approximation convention as `sql.rs`'s
    // `atomic_transaction_wrapped_toggle_is_not_flagged`): the `find` + `.create(` trigger shape is still
    // present, but a `.upsert(` call present anywhere in the same function is treated as proof the
    // duplicate-row race has a real fix in place (unlike a bare `$transaction` wrap, which is no longer
    // accepted as one), so the `absent` veto suppresses the finding.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function ensureUserUpsert(email: string) {\n  const existing = await prisma.user.findFirst({ where: { email } });\n  if (!existing) {\n    await prisma.user.create({ data: { email } });\n  }\n  await prisma.userProfile.upsert({ where: { email }, create: { email }, update: {} });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "find-then-create-no-unique").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn find_create_ok_marker_directly_above_the_create_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function ensureUserMarked(email: string) {\n  const existing = await prisma.user.findFirst({ where: { email } });\n  if (!existing) {\n    // find-create-ok: low-traffic admin-only path, race window accepted\n    await prisma.user.create({ data: { email } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "find-then-create-no-unique").is_empty(),
        "{:?}",
        out.findings
    );
}

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

// --- skip_comment_lines + test-path file_exclude_pattern ---
// A commented-out bulk-write call must not fire `update-delete-no-where`, and every DB-write rule in this
// pack excludes test-fixture paths (a test writing against a mock DB is not a production bug).

#[test]
fn update_many_call_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function bulkArchive() {\n  // prisma.order.updateMany({ data: { archived: true } }) -- old approach, replaced below\n  return 0;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn take_sourced_from_req_query_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/__tests__/service.test.ts",
        "declare const prisma: any;\ndeclare const req: any;\nexport async function listUsers() {\n  return prisma.user.findMany({ take: Number(req.query.limit) });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unbounded-user-limit").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- multi-write-no-tx ---

#[test]
fn create_then_update_with_no_transaction_in_same_function_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function checkoutAndArchive(id: string) {\n  await prisma.order.create({ data: { id } });\n  await prisma.order.update({ where: { id }, data: { archived: true } });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "multi-write-no-tx");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn create_then_update_wrapped_in_prisma_transaction_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function checkoutAndArchiveTx(id: string) {\n  await prisma.$transaction(async (tx: any) => {\n    await tx.order.create({ data: { id } });\n    await tx.order.update({ where: { id }, data: { archived: true } });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "multi-write-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn create_then_update_wrapped_in_a_bare_transaction_call_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function transaction(fn: () => Promise<void>): Promise<void>;\nexport async function checkoutAndArchiveBareTx(id: string) {\n  await transaction(async () => {\n    await prisma.order.create({ data: { id } });\n    await prisma.order.update({ where: { id }, data: { archived: true } });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "multi-write-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn create_then_update_guarded_by_a_quoted_begin_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const db: any;\nexport async function checkoutAndArchiveBegin(id: string) {\n  await db.query(\"BEGIN\");\n  await db.order.create({ data: { id } });\n  await db.order.update({ where: { id }, data: { archived: true } });\n  await db.query(\"COMMIT\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "multi-write-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn only_one_write_family_present_is_not_flagged() {
    // The co-occurrence requirement: `mutate-write` alone, with no `create-write` anywhere in the file,
    // never satisfies the whole-file necessary-condition pre-skip, let alone the per-span check.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function archiveOnly(id: string) {\n  await prisma.order.update({ where: { id }, data: { archived: true } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "multi-write-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn multi_write_tx_ok_marker_directly_above_the_mutate_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function checkoutAndArchiveMarked(id: string) {\n  await prisma.order.create({ data: { id } });\n  // multi-write-tx-ok: archive failure is acceptable, the order is already recorded\n  await prisma.order.update({ where: { id }, data: { archived: true } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "multi-write-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- non-atomic-counter-update ---

#[test]
fn find_unique_then_arithmetic_update_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function incrementViews(id: string) {\n  const post = await prisma.post.findUnique({ where: { id } });\n  await prisma.post.update({ where: { id }, data: { views: post.views + 1 } });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "non-atomic-counter-update");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn atomic_increment_guard_is_not_flagged() {
    // Veto-mechanism test: an unrelated arithmetic-shaped expression still satisfies the `arith-update`
    // co-occurrence trigger, but the real update uses Prisma's atomic `{ increment: 1 }`, so the
    // `atomic-increment` absent guard suppresses the finding.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function incrementViewsAtomicGuard(id: string) {\n  const post = await prisma.post.findUnique({ where: { id } });\n  const preview = { views: post.views + 1 };\n  await prisma.post.update({ where: { id }, data: { views: { increment: 1 } } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "non-atomic-counter-update").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn atomic_decrement_guard_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function decrementStockAtomicGuard(id: string) {\n  const item = await prisma.item.findFirst({ where: { id } });\n  const preview = { stock: item.stock - 1 };\n  await prisma.item.update({ where: { id }, data: { stock: { decrement: 1 } } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "non-atomic-counter-update").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mongo_style_inc_guard_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const collection: any;\nexport async function incrementCounterMongo(id: string) {\n  const doc = await collection.findOne({ id });\n  const preview = { count: doc.count + 1 };\n  await collection.updateOne({ id }, { $inc: { count: 1 } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "non-atomic-counter-update").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn select_for_update_row_lock_guard_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const db: any;\nexport async function incrementBalanceRowLock(id: string) {\n  const row = await db.findFirst({ where: { id } });\n  const preview = { balance: row.balance + 1 };\n  await db.query(\"SELECT * FROM accounts WHERE id = $1 FOR UPDATE\", [id]);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "non-atomic-counter-update").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn atomic_counter_ok_marker_directly_above_the_arithmetic_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function incrementViewsMarked(id: string) {\n  const post = await prisma.post.findUnique({ where: { id } });\n  // atomic-counter-ok: single-writer batch job, no concurrent access possible\n  await prisma.post.update({ where: { id }, data: { views: post.views + 1 } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "non-atomic-counter-update").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- connection-no-release ---

#[test]
fn pool_connect_with_no_release_anywhere_in_function_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function runQuery(sql: string) {\n  const conn = await pool.connect();\n  await conn.query(sql);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "connection-no-release");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn get_connection_with_no_release_anywhere_in_function_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function runQueryGetConnection(sql: string) {\n  const conn = await pool.getConnection();\n  await conn.query(sql);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "connection-no-release");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn pool_connect_released_in_finally_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function runQueryReleased(sql: string) {\n  const conn = await pool.connect();\n  try {\n    await conn.query(sql);\n  } finally {\n    conn.release();\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "connection-no-release").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pool_connect_returned_to_caller_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function acquireConnectionForCaller() {\n  const conn = await pool.connect();\n  return conn;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "connection-no-release").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pool_connect_bound_via_await_using_declaration_is_not_flagged() {
    // Review calibration pin: TS explicit resource management (`using` / `await using`,
    // Symbol.dispose) releases on scope exit — the message promises this shape is recognized.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function withManagedConnection() {\n  await using conn = await pool.connect();\n  await conn.query('SELECT 1');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "connection-no-release").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pool_connect_released_only_inside_a_helper_function_is_still_flagged() {
    // Documents the message's own honest limitation: `MethodScan`'s `absent` veto only sees text inside
    // THIS function's own span. `releaseConn(conn)` is a bare identifier call, not `.release(`/`.destroy(`/
    // `.end(`, so it never satisfies the `released` veto even though the connection genuinely is released,
    // one call away, inside `releaseConn`. This is a real false positive the message warns readers about
    // ("verify before refactoring") rather than a bug — pinned here so the claim can't silently drift.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\ndeclare function releaseConn(conn: any): void;\nexport async function runQueryHelperRelease(sql: string) {\n  const conn = await pool.connect();\n  await conn.query(sql);\n  releaseConn(conn);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "connection-no-release");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn connection_release_ok_marker_directly_above_the_acquire_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function runQueryMarked(sql: string) {\n  // connection-release-ok: pooled test harness connection, released by the test teardown hook\n  const conn = await pool.connect();\n  await conn.query(sql);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "connection-no-release").is_empty(),
        "{:?}",
        out.findings
    );
}
