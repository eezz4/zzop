//! `external-call-in-tx` + `multi-write-no-tx` tests (split from `be-db.rs`).

use super::*;

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
