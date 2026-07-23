//! `external-call-in-tx` + `multi-write-no-tx` + `write-in-loop-no-tx` + `unawaited-transaction` +
//! `manual-tx-no-rollback` + `tx-swallows-error-commits` + `critical-write-default-isolation` +
//! `tx-in-loop-long-hold` tests (split from `be-db.rs`).

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

// --- write-in-loop-no-tx ---

#[test]
fn update_call_inside_for_of_loop_with_no_transaction_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const users: { id: string }[];\nexport async function activateAll() {\n  for (const u of users) {\n    await prisma.account.update({ where: { id: u.id }, data: { active: true } });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "write-in-loop-no-tx");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

#[test]
fn read_only_loop_is_not_a_write_in_loop_finding() {
    // `findMany` is not a write verb, so the trigger pattern never matches inside the loop span at all.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const ids: string[];\nexport async function readAll() {\n  for (const id of ids) {\n    await prisma.user.findMany({ where: { id } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "write-in-loop-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn write_in_loop_wrapped_in_transaction_is_not_flagged() {
    // The loop body's write is present, but `$transaction(` sits in the same enclosing function span,
    // so the `prisma-tx` absent-veto suppresses the finding.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const users: { id: string }[];\nexport async function activateAllTx() {\n  await prisma.$transaction(async (tx: any) => {\n    for (const u of users) {\n      await tx.account.update({ where: { id: u.id }, data: { active: true } });\n    }\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "write-in-loop-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn write_in_loop_ok_marker_directly_above_the_write_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const users: { id: string }[];\nexport async function activateAllMarked() {\n  for (const u of users) {\n    // write-in-loop-ok: idempotent per-row activation flag, safe to autocommit\n    await prisma.account.update({ where: { id: u.id }, data: { active: true } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "write-in-loop-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- unawaited-transaction ---

#[test]
fn bare_statement_transaction_call_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const op1: any;\ndeclare const op2: any;\nexport async function checkoutBatch() {\n  prisma.$transaction([op1, op2]);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-transaction");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

#[test]
fn awaited_transaction_assigned_to_a_variable_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const op1: any;\ndeclare const op2: any;\nexport async function checkoutBatchAwaited() {\n  const r = await prisma.$transaction([op1, op2]);\n  return r;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-transaction").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn returned_transaction_call_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function fn(): Promise<void>;\nexport async function checkoutBatchReturned() {\n  return prisma.$transaction(fn);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-transaction").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn catch_chained_transaction_call_is_not_flagged() {
    // The message's remedy list promises `.catch()` clears — v0.21.0 release-audit (message lens)
    // caught the exclude_pattern missing the `\.catch\b` alternative; this pin seals the repair.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const op1: any;\ndeclare const op2: any;\nexport async function checkoutBatchCaught() {\n  prisma.$transaction([op1, op2]).catch(reportTxFailure);\n}\ndeclare function reportTxFailure(e: unknown): void;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-transaction").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn unawaited_tx_ok_marker_directly_above_the_transaction_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const op1: any;\ndeclare const op2: any;\nexport async function checkoutBatchMarked() {\n  // unawaited-tx-ok: fire-and-forget audit transaction, failure acceptable\n  prisma.$transaction([op1, op2]);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-transaction").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- manual-tx-no-rollback ---

#[test]
fn manual_begin_and_commit_with_no_rollback_anywhere_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const client: any;\nexport async function transferFunds(from: string, to: string, amount: number) {\n  await client.query(\"BEGIN\");\n  await client.query(\"INSERT INTO ledger (acct, amount) VALUES ($1, $2)\", [from, -amount]);\n  await client.query(\"COMMIT\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "manual-tx-no-rollback");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn manual_begin_commit_with_a_rollback_in_catch_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const client: any;\nexport async function transferFundsSafe(from: string, to: string, amount: number) {\n  try {\n    await client.query(\"BEGIN\");\n    await client.query(\"INSERT INTO ledger (acct, amount) VALUES ($1, $2)\", [from, -amount]);\n    await client.query(\"COMMIT\");\n  } catch (e) {\n    await client.query(\"ROLLBACK\");\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "manual-tx-no-rollback").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn orm_managed_transaction_with_no_literal_begin_or_commit_is_not_flagged() {
    // No literal "BEGIN"/"COMMIT" text anywhere in the file, so the whole-file necessary-condition
    // pre-skip never even reaches the per-span check — an ORM-managed `$transaction` is out of scope
    // for this rule by design (it narrows to manual BEGIN/COMMIT transactions).
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function transferFundsOrm(from: string, to: string, amount: number) {\n  await prisma.$transaction(async (tx: any) => {\n    await tx.ledger.create({ data: { acct: from, amount: -amount } });\n    await tx.ledger.create({ data: { acct: to, amount } });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "manual-tx-no-rollback").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn manual_tx_ok_marker_directly_above_the_begin_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const client: any;\nexport async function transferFundsMarked(from: string, to: string, amount: number) {\n  // manual-tx-ok: legacy migration script, rollback handled by the caller's outer transaction\n  await client.query(\"BEGIN\");\n  await client.query(\"INSERT INTO ledger (acct, amount) VALUES ($1, $2)\", [from, -amount]);\n  await client.query(\"COMMIT\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "manual-tx-no-rollback").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- tx-swallows-error-commits ---

#[test]
fn empty_catch_inside_a_transaction_callback_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function credit(tx: any, amount: number): Promise<void>;\nexport async function payout(amount: number) {\n  await prisma.$transaction(async (tx: any) => {\n    try {\n      await credit(tx, amount);\n    } catch {}\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "tx-swallows-error-commits");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 7);
}

#[test]
fn catch_that_rethrows_inside_a_transaction_callback_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function credit(tx: any, amount: number): Promise<void>;\nexport async function payoutRethrow(amount: number) {\n  await prisma.$transaction(async (tx: any) => {\n    try {\n      await credit(tx, amount);\n    } catch (e) {\n      throw e;\n    }\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-swallows-error-commits").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn empty_catch_with_no_transaction_anywhere_in_the_function_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare function riskyOp(): Promise<void>;\nexport async function bestEffort() {\n  try {\n    await riskyOp();\n  } catch {}\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-swallows-error-commits").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_catch_ok_marker_directly_above_the_empty_catch_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function credit(tx: any, amount: number): Promise<void>;\nexport async function payoutMarked(amount: number) {\n  await prisma.$transaction(async (tx: any) => {\n    try {\n      await credit(tx, amount);\n      // tx-catch-ok: credit failure intentionally ignored, payout already recorded elsewhere\n    } catch {}\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-swallows-error-commits").is_empty(),
        "{:?}",
        out.findings
    );
}

// Regression (opus review F1): the interactive-callback form is the primary target — the old
// `exclude_pattern` carried `=>`, which the `$transaction(async (tx) => {` opening line always contains,
// so every detached interactive transaction was silently excluded and only the array form ever fired.
#[test]
fn detached_interactive_transaction_callback_form_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const id: string;\nexport async function transfer() {\n  prisma.$transaction(async (tx: any) => {\n    await tx.account.update({ where: { id }, data: { active: true } });\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-transaction");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

// The awaited interactive form must still NOT fire (the `await` on the opening line vetoes it).
#[test]
fn awaited_interactive_transaction_callback_form_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const id: string;\nexport async function transfer() {\n  await prisma.$transaction(async (tx: any) => {\n    await tx.account.update({ where: { id }, data: { active: true } });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-transaction").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- critical-write-default-isolation ---

#[test]
fn money_tx_with_no_explicit_isolation_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const amt: number;\nexport async function transferFunds(accountId: string) {\n  await prisma.$transaction(async (tx: any) => {\n    const acct = await tx.account.findUnique({ where: { id: accountId } });\n    await tx.account.update({ where: { id: accountId }, data: { balance: acct.balance - amt } });\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "critical-write-default-isolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn money_tx_with_explicit_serializable_isolation_is_not_flagged() {
    // Veto-mechanism test: the tx/money/write co-occurrence trigger shape is still present, but the
    // `$transaction` call is given an explicit `isolationLevel`, so the `isolation` absent guard suppresses
    // the finding.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const amt: number;\nexport async function transferFundsSerializable(accountId: string) {\n  await prisma.$transaction(async (tx: any) => {\n    const acct = await tx.account.findUnique({ where: { id: accountId } });\n    await tx.account.update({ where: { id: accountId }, data: { balance: acct.balance - amt } });\n  }, { isolationLevel: Prisma.TransactionIsolationLevel.Serializable });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "critical-write-default-isolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_over_non_money_model_is_not_flagged() {
    // The whole-file necessary-condition pre-skip: no money-named identifier appears anywhere in the file,
    // so the `money` pattern never matches, regardless of the `tx` + `write` co-occurrence.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function recordEvent(eventId: string, payload: string) {\n  await prisma.$transaction(async (tx: any) => {\n    const existing = await tx.event.findUnique({ where: { id: eventId } });\n    await tx.event.update({ where: { id: eventId }, data: { payload } });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "critical-write-default-isolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_isolation_ok_marker_directly_above_the_transaction_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const amt: number;\nexport async function transferFundsMarked(accountId: string) {\n  // tx-isolation-ok: single-writer offline batch job, no concurrent access possible\n  await prisma.$transaction(async (tx: any) => {\n    const acct = await tx.account.findUnique({ where: { id: accountId } });\n    await tx.account.update({ where: { id: accountId }, data: { balance: acct.balance - amt } });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "critical-write-default-isolation").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- tx-in-loop-long-hold ---

#[test]
fn tx_call_inside_for_of_loop_within_a_transaction_is_flagged() {
    // Same fixture shape as `write_in_loop_wrapped_in_transaction_is_not_flagged` above (the tx-wrap
    // veto suppresses `write-in-loop-no-tx` there) — here the presence of the SAME `$transaction(` wrap
    // is exactly what this rule requires, so the two rules are mutually exclusive on this line.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const users: { id: string }[];\nexport async function activateAllTx() {\n  await prisma.$transaction(async (tx: any) => {\n    for (const u of users) {\n      await tx.account.update({ where: { id: u.id }, data: { active: true } });\n    }\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "tx-in-loop-long-hold");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
    // Mirror-image assertion: `write-in-loop-no-tx` must NOT fire on this same fixture (the tx-wrap
    // veto suppresses it), proving the two rules never co-fire on the same line.
    assert!(
        hits(&out, "write-in-loop-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn write_in_loop_with_no_transaction_does_not_flag_tx_in_loop_long_hold() {
    // Mirror image of the above: no `$transaction(` anywhere in the file, so the whole-file
    // necessary-condition pre-skip never even reaches the per-span check regardless of the loop-proven
    // write call. This is `write-in-loop-no-tx`'s territory instead (see
    // `update_call_inside_for_of_loop_with_no_transaction_is_flagged` above).
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const users: { id: string }[];\nexport async function activateAll() {\n  for (const u of users) {\n    await prisma.account.update({ where: { id: u.id }, data: { active: true } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-in-loop-long-hold").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_call_outside_any_loop_within_a_transaction_is_not_flagged() {
    // The `$transaction(` wrap is present, but the `tx.account.update(...)` call sits directly in the
    // callback body with no enclosing loop — `trigger_in_loop` never satisfies, since a single call per
    // transaction invocation holds the lock for one row, not N.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function activateOneTx(id: string) {\n  await prisma.$transaction(async (tx: any) => {\n    await tx.account.update({ where: { id }, data: { active: true } });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-in-loop-long-hold").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_loop_hold_ok_marker_directly_above_the_tx_call_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const users: { id: string }[];\nexport async function activateAllTxMarked() {\n  await prisma.$transaction(async (tx: any) => {\n    for (const u of users) {\n      // tx-loop-hold-ok: bounded fixture list, at most 5 rows per invocation\n      await tx.account.update({ where: { id: u.id }, data: { active: true } });\n    }\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-in-loop-long-hold").is_empty(),
        "{:?}",
        out.findings
    );
}
