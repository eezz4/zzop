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
