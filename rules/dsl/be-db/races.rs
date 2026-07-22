//! `find-then-create-no-unique` + `non-atomic-counter-update` race tests (split from `be-db.rs`).

use super::*;

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
    // still races and must fire.
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

// --- check-then-act-in-loop ---

#[test]
fn find_first_then_create_both_inside_for_of_loop_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const rows: { key: string }[];\nexport async function ensureAll() {\n  for (const r of rows) {\n    const e = await prisma.item.findFirst({ where: { key: r.key } });\n    if (!e) {\n      await prisma.item.create({ data: r });\n    }\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "check-then-act-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn find_first_in_loop_but_create_after_the_loop_closes_is_not_flagged() {
    // The `.create(` call sits AFTER the loop's closing brace, outside every projected loop span, so
    // `trigger_in_loop` never satisfies even though `findFirst` is genuinely in-loop.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const rows: { key: string }[];\ndeclare const acc: any[];\nexport async function collectThenCreateOnce() {\n  for (const r of rows) {\n    const e = await prisma.item.findFirst({ where: { key: r.key } });\n    acc.push(e);\n  }\n  await prisma.item.create({ data: acc[0] });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "check-then-act-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn loop_using_upsert_instead_of_find_then_create_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const rows: { key: string }[];\nexport async function ensureAllUpsert() {\n  for (const r of rows) {\n    await prisma.item.upsert({ where: { key: r.key }, create: r, update: {} });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "check-then-act-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn check_act_loop_ok_marker_directly_above_the_create_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const rows: { key: string }[];\nexport async function ensureAllMarked() {\n  for (const r of rows) {\n    const e = await prisma.item.findFirst({ where: { key: r.key } });\n    if (!e) {\n      // check-act-loop-ok: single-threaded seed script, no concurrent workers\n      await prisma.item.create({ data: r });\n    }\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "check-then-act-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- idempotency-key-regenerated-per-retry ---

#[test]
fn idempotency_key_regenerated_via_random_uuid_inside_retry_loop_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/payments.ts",
        "declare const attempts: number[];\ndeclare const body: any;\ndeclare const api: any;\ndeclare function randomUUID(): string;\nexport async function chargeWithRetries() {\n  for (const attempt of attempts) {\n    const idempotencyKey = randomUUID();\n    await api.post(\"/charge\", body, { headers: { \"Idempotency-Key\": idempotencyKey } });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "idempotency-key-regenerated-per-retry");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 7);
}

/// FP-adversarial (nearest harmless lookalike): the key is still generated with `randomUUID()`, but the
/// assignment sits BEFORE the loop and is reused across every attempt — the assignment's own line never
/// falls inside the loop span, so `trigger_in_loop` never satisfies.
#[test]
fn idempotency_key_generated_once_before_loop_and_reused_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/payments.ts",
        "declare const attempts: number[];\ndeclare const body: any;\ndeclare const api: any;\ndeclare function randomUUID(): string;\nexport async function chargeOnceKey() {\n  const idempotencyKey = randomUUID();\n  for (const attempt of attempts) {\n    await api.post(\"/charge\", body, { headers: { \"Idempotency-Key\": idempotencyKey } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "idempotency-key-regenerated-per-retry").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn idempotency_key_derived_deterministically_inside_loop_is_not_flagged() {
    // `hash(o.id)` is not one of the recognized random-generator calls, so the trigger pattern never
    // matches this line at all.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/payments.ts",
        "declare const orders: { id: string }[];\ndeclare const api: any;\ndeclare function hash(id: string): string;\nexport async function chargeOrders() {\n  for (const o of orders) {\n    const idempotencyKey = hash(o.id);\n    await api.post(\"/charge\", o, { headers: { \"Idempotency-Key\": idempotencyKey } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "idempotency-key-regenerated-per-retry").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn idempotency_regen_ok_marker_directly_above_the_regenerated_key_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/payments.ts",
        "declare const attempts: number[];\ndeclare const body: any;\ndeclare const api: any;\ndeclare function randomUUID(): string;\nexport async function chargeWithRetriesMarked() {\n  for (const attempt of attempts) {\n    // idempotency-regen-ok: sandbox test harness, retries treated as new charges intentionally\n    const idempotencyKey = randomUUID();\n    await api.post(\"/charge\", body, { headers: { \"Idempotency-Key\": idempotencyKey } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "idempotency-key-regenerated-per-retry").is_empty(),
        "{:?}",
        out.findings
    );
}
