//! `external-call-and-tx` + `multi-write-no-tx` + `write-in-loop-no-tx` + `unawaited-transaction` +
//! `manual-tx-no-rollback` + `tx-and-empty-catch` + `money-tx-no-isolation-level` +
//! `tx-and-db-call-in-loop` tests (split from `db.rs`).

use super::*;

// --- external-call-and-tx ---

#[test]
fn fetch_call_inside_transaction_callback_is_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function fetch(url: string, init?: any): Promise<any>;\nexport async function checkoutOrder(orderId: string) {\n  await prisma.$transaction(async (tx: any) => {\n    await tx.order.update({ where: { id: orderId }, data: { status: \"paid\" } });\n    await fetch(\"https://payments.example.com/notify\", { method: \"POST\" });\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "external-call-and-tx");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

#[test]
fn fetch_call_with_no_transaction_in_function_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function fetch(url: string, init?: any): Promise<any>;\nexport async function checkoutOrderSafe(orderId: string) {\n  const paymentResult = await fetch(\"https://payments.example.com/notify\", { method: \"POST\" });\n  await prisma.order.update({ where: { id: orderId }, data: { status: \"paid\" } });\n  return paymentResult;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "external-call-and-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_egress_ok_marker_directly_above_the_fetch_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function fetch(url: string, init?: any): Promise<any>;\nexport async function checkoutOrderMarked(orderId: string) {\n  await prisma.$transaction(async (tx: any) => {\n    await tx.order.update({ where: { id: orderId }, data: { status: \"paid\" } });\n    // zzop-external-call-and-tx-ok: payment gateway called via idempotent webhook retry, safe inside tx\n    await fetch(\"https://payments.example.com/notify\", { method: \"POST\" });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "external-call-and-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- multi-write-no-tx ---

#[test]
fn create_then_update_with_no_transaction_in_same_function_is_flagged() {
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function checkoutAndArchiveMarked(id: string) {\n  await prisma.order.create({ data: { id } });\n  // zzop-multi-write-no-tx-ok: archive failure is acceptable, the order is already recorded\n  await prisma.order.update({ where: { id }, data: { archived: true } });\n}\n",
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const users: { id: string }[];\nexport async function activateAllMarked() {\n  for (const u of users) {\n    // zzop-write-in-loop-no-tx-ok: idempotent per-row activation flag, safe to autocommit\n    await prisma.account.update({ where: { id: u.id }, data: { active: true } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "write-in-loop-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- the counter-indication the remedy has to carry (2026-08-25) ---
//
// The prescription used to open with the imperative "Wrap the loop body in `$transaction(...)`", and a
// blind auditor who followed it on cal.com apps/web/app/api/cron/bookingReminder/route.ts would have
// broken that cron twice over: the loop SENDS (`sendOrganizerRequestReminderEmail`, :144) before it
// RECORDS (`prisma.reminderMail.create`, :146), so a Prisma interactive transaction blows its 5s default
// on the SMTP call and, when it rolls back, deletes the rows proving the mail already went out — the next
// run re-sends the whole backlog. The per-iteration autocommit there is the CORRECT at-least-once
// ordering, not an oversight.
//
// This is the same pin `rules/dsl/security/crypto.rs`'s `an_hmac_sha1_construction_still_fires_and_says_
// it_is_not_a_bare_digest` puts on `security/weak-crypto`'s counterparty-fixed disclosure, for the same
// reason: prose in a design document stops nothing, a needle in the DELIVERED message does. The rule must
// keep firing (the hazard is real when the body is pure) while the message stops the reader who is not in
// that case — condition first, imperative second.

#[test]
fn write_in_loop_message_puts_the_side_effect_condition_before_the_transaction_imperative() {
    let dir = TempDir::new("zzop-db");
    // The bookingReminder shape, reduced: send, then record, once per booking, no transaction.
    dir.write(
        "src/cron.ts",
        "declare const prisma: any;\ndeclare const bookings: { id: string }[];\ndeclare function sendReminderEmail(id: string): Promise<void>;\nexport async function remind() {\n  for (const b of bookings) {\n    await sendReminderEmail(b.id);\n    await prisma.reminderMail.create({ data: { referenceId: b.id } });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "write-in-loop-no-tx");
    // The finding STILL FIRES. This repair is a sentence repair: nothing is suppressed, vetoed or
    // downgraded, and a count that moves here means the contract was broken.
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 7);

    let m = &h[0].message;
    for needle in [
        // the condition, and the fact that it is REACHED FIRST (asserted by offset below)
        "IF THE LOOP BODY TOUCHES NOTHING OUTSIDE THE DATABASE",
        "DO NOT WRAP IT",
        // THE mechanism. Two were claimed until 2026-08-25 and only this one survived being checked
        // at the site the message was written for: `sendOrganizerRequestReminderEmail`
        // (cal.com packages/emails/email-manager.ts:562-580) pushes into a local array and returns
        // WITHOUT awaiting it — the sibling function 2 lines below ends with `await
        // Promise.all(emailsToSend)` and this one does not — so the `await` at route.ts:144 resolves
        // on the next microtask and a `$transaction` wrap would very likely fit inside Prisma's 5s
        // default. The retired sentence claimed that timeout as a coequal mechanism; it did not fire
        // here, and its detail bought the message credence it had not earned. Cut, not softened.
        "the rollback deletes the record while the side effect itself is already gone",
        "re-sends its entire backlog",
        // the ordering the current code is already getting RIGHT
        "at-least-once",
        // Prescribe-after-checking. The remedy used to tell the reader to BUILD replay-safety; at the
        // anchor site route.ts:83-93 already reads `reminderMail` and :95 filters the loop against it,
        // 51 lines above the finding. A message that prescribes a defence must first ask whether the
        // file already has one.
        "CHECK WHETHER THE LOOP ALREADY HAS IT",
    ] {
        assert!(
            m.contains(needle),
            "write-in-loop-no-tx's remedy no longer carries its counter-indication — missing \
             {needle:?}. A reader whose loop body sends mail before recording it follows the \
             transaction advice and ships a cron that re-sends every reminder. In: {m}"
        );
    }

    // ORDER IS THE FIX, not merely presence. Cheat-sheet §9.5: an imperative that leads gets quoted and
    // the qualifier 900 characters later gets missed — measured on three independent auditors. So the
    // condition must be reached BEFORE the first `$transaction(...)` imperative, not just exist.
    let condition = m
        .find("IF THE LOOP BODY TOUCHES NOTHING OUTSIDE THE DATABASE")
        .expect("needle asserted above");
    let imperative = m
        .find("wrap the loop body in `$transaction(...)`")
        .expect("the remedy must still name the transaction fix for the pure-body case");
    assert!(
        condition < imperative,
        "the `$transaction(...)` imperative is reached before the side-effect condition that \
         invalidates it (condition at {condition}, imperative at {imperative}) — the shape the \
         message was rewritten out of. In: {m}"
    );
}

// --- unawaited-transaction ---

#[test]
fn bare_statement_transaction_call_is_flagged() {
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const op1: any;\ndeclare const op2: any;\nexport async function checkoutBatchMarked() {\n  // zzop-unawaited-transaction-ok: fire-and-forget audit transaction, failure acceptable\n  prisma.$transaction([op1, op2]);\n}\n",
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const client: any;\nexport async function transferFundsMarked(from: string, to: string, amount: number) {\n  // zzop-manual-tx-no-rollback-ok: legacy migration script, rollback handled by the caller's outer transaction\n  await client.query(\"BEGIN\");\n  await client.query(\"INSERT INTO ledger (acct, amount) VALUES ($1, $2)\", [from, -amount]);\n  await client.query(\"COMMIT\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "manual-tx-no-rollback").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- tx-and-empty-catch ---

#[test]
fn empty_catch_inside_a_transaction_callback_is_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function credit(tx: any, amount: number): Promise<void>;\nexport async function payout(amount: number) {\n  await prisma.$transaction(async (tx: any) => {\n    try {\n      await credit(tx, amount);\n    } catch {}\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "tx-and-empty-catch");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 7);
}

#[test]
fn catch_that_rethrows_inside_a_transaction_callback_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function credit(tx: any, amount: number): Promise<void>;\nexport async function payoutRethrow(amount: number) {\n  await prisma.$transaction(async (tx: any) => {\n    try {\n      await credit(tx, amount);\n    } catch (e) {\n      throw e;\n    }\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-and-empty-catch").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn empty_catch_with_no_transaction_anywhere_in_the_function_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare function riskyOp(): Promise<void>;\nexport async function bestEffort() {\n  try {\n    await riskyOp();\n  } catch {}\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-and-empty-catch").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_catch_ok_marker_directly_above_the_empty_catch_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function credit(tx: any, amount: number): Promise<void>;\nexport async function payoutMarked(amount: number) {\n  await prisma.$transaction(async (tx: any) => {\n    try {\n      await credit(tx, amount);\n      // zzop-tx-and-empty-catch-ok: credit failure intentionally ignored, payout already recorded elsewhere\n    } catch {}\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-and-empty-catch").is_empty(),
        "{:?}",
        out.findings
    );
}

// Regression (opus review F1): the interactive-callback form is the primary target — the old
// `exclude_pattern` carried `=>`, which the `$transaction(async (tx) => {` opening line always contains,
// so every detached interactive transaction was silently excluded and only the array form ever fired.
#[test]
fn detached_interactive_transaction_callback_form_is_flagged() {
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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

// --- money-tx-no-isolation-level ---

#[test]
fn money_tx_with_no_explicit_isolation_is_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const amt: number;\nexport async function transferFunds(accountId: string) {\n  await prisma.$transaction(async (tx: any) => {\n    const acct = await tx.account.findUnique({ where: { id: accountId } });\n    await tx.account.update({ where: { id: accountId }, data: { balance: acct.balance - amt } });\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "money-tx-no-isolation-level");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn money_tx_with_explicit_serializable_isolation_is_not_flagged() {
    // Veto-mechanism test: the tx/money/write co-occurrence trigger shape is still present, but the
    // `$transaction` call is given an explicit `isolationLevel`, so the `isolation` absent guard suppresses
    // the finding.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const amt: number;\nexport async function transferFundsSerializable(accountId: string) {\n  await prisma.$transaction(async (tx: any) => {\n    const acct = await tx.account.findUnique({ where: { id: accountId } });\n    await tx.account.update({ where: { id: accountId }, data: { balance: acct.balance - amt } });\n  }, { isolationLevel: Prisma.TransactionIsolationLevel.Serializable });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "money-tx-no-isolation-level").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_over_non_money_model_is_not_flagged() {
    // The whole-file necessary-condition pre-skip: no money-named identifier appears anywhere in the file,
    // so the `money` pattern never matches, regardless of the `tx` + `write` co-occurrence.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function recordEvent(eventId: string, payload: string) {\n  await prisma.$transaction(async (tx: any) => {\n    const existing = await tx.event.findUnique({ where: { id: eventId } });\n    await tx.event.update({ where: { id: eventId }, data: { payload } });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "money-tx-no-isolation-level").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_isolation_ok_marker_directly_above_the_transaction_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const amt: number;\nexport async function transferFundsMarked(accountId: string) {\n  // zzop-money-tx-no-isolation-level-ok: single-writer offline batch job, no concurrent access possible\n  await prisma.$transaction(async (tx: any) => {\n    const acct = await tx.account.findUnique({ where: { id: accountId } });\n    await tx.account.update({ where: { id: accountId }, data: { balance: acct.balance - amt } });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "money-tx-no-isolation-level").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- tx-and-db-call-in-loop ---

#[test]
fn tx_call_inside_for_of_loop_within_a_transaction_is_flagged() {
    // Same fixture shape as `write_in_loop_wrapped_in_transaction_is_not_flagged` above (the tx-wrap
    // veto suppresses `write-in-loop-no-tx` there) — here the presence of the SAME `$transaction(` wrap
    // is exactly what this rule requires, so the two rules are mutually exclusive on this line.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const users: { id: string }[];\nexport async function activateAllTx() {\n  await prisma.$transaction(async (tx: any) => {\n    for (const u of users) {\n      await tx.account.update({ where: { id: u.id }, data: { active: true } });\n    }\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "tx-and-db-call-in-loop");
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
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const users: { id: string }[];\nexport async function activateAll() {\n  for (const u of users) {\n    await prisma.account.update({ where: { id: u.id }, data: { active: true } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-and-db-call-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_call_outside_any_loop_within_a_transaction_is_not_flagged() {
    // The `$transaction(` wrap is present, but the `tx.account.update(...)` call sits directly in the
    // callback body with no enclosing loop — `trigger_in_loop` never satisfies, since a single call per
    // transaction invocation holds the lock for one row, not N.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function activateOneTx(id: string) {\n  await prisma.$transaction(async (tx: any) => {\n    await tx.account.update({ where: { id }, data: { active: true } });\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-and-db-call-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tx_loop_hold_ok_marker_directly_above_the_tx_call_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const users: { id: string }[];\nexport async function activateAllTxMarked() {\n  await prisma.$transaction(async (tx: any) => {\n    for (const u of users) {\n      // zzop-tx-and-db-call-in-loop-ok: bounded fixture list, at most 5 rows per invocation\n      await tx.account.update({ where: { id: u.id }, data: { active: true } });\n    }\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "tx-and-db-call-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

/// §27 ordering pin (2026-08-26). This rule is a CO-OCCURRENCE heuristic and says so — but it said so
/// ~420 bytes behind "Move the network call outside the `$transaction` block", and the shape it names
/// (a `$transaction` that has already COMMITTED, with an unrelated `fetch` on the next line) holds no lock
/// across anything. The remedy now carries the premise. Nothing about the match moved: same trigger, same
/// veto, same `warning`.
#[test]
fn external_call_and_tx_message_puts_the_lock_hold_condition_before_the_move_imperative() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare function fetch(url: string, init?: any): Promise<any>;\nexport async function checkoutOrder(orderId: string) {\n  await prisma.$transaction(async (tx: any) => {\n    await tx.order.update({ where: { id: orderId }, data: { status: \"paid\" } });\n    await fetch(\"https://payments.example.com/notify\", { method: \"POST\" });\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "external-call-and-tx");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_disqualifier_summary_precedes_imperative(
        "external-call-and-tx",
        &h[0].message,
        "IF THE NETWORK CALL REALLY HAPPENS WHILE THE TRANSACTION IS OPEN",
        "move the network call outside",
        "the transaction has already COMMITTED",
    );
}

/// §27 pin (2026-08-27). This rule already put its own disqualifier ("Treat this as a review nudge")
/// ahead of the imperative, so the OPEN leg was not order but silence: the message said "Set an
/// explicit isolation level" and never said what raising it does.
///
/// Postgres does not make the second writer wait under `Serializable`/`RepeatableRead` — it ABORTS one
/// of the two with `40001`, which Prisma surfaces as `P2034`, and Prisma has no retry of its own. So a
/// money path that was silently racing starts throwing under concurrency, which on the measured example
/// (cal.com `packages/app-store/_utils/payments/handlePaymentSuccess.ts`) is a Stripe webhook handler:
/// it 500s, the sender re-delivers, and the retry lands on a transaction that may already have
/// committed. `retry`, `40001` and `conflict` occurred ZERO times in the shipped message.
///
/// The `FOR UPDATE` exit was already in the message and is now stated as the branch that needs NO retry
/// path — a remedy must be followable to a green state, and for a reader with no retry infrastructure
/// the row lock is the one that is. Detection untouched: same trigger, same veto, same `info`.
#[test]
fn money_tx_message_lands_the_serialization_failure_before_the_isolation_imperative() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const amt: number;\nexport async function transferFunds(accountId: string) {\n  await prisma.$transaction(async (tx: any) => {\n    const acct = await tx.account.findUnique({ where: { id: accountId } });\n    await tx.account.update({ where: { id: accountId }, data: { balance: acct.balance - amt } });\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "money-tx-no-isolation-level");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
    assert_disqualifier_summary_precedes_imperative(
        "money-tx-no-isolation-level",
        &h[0].message,
        "RAISING THE LEVEL IS NOT FREE",
        "Set an explicit isolation level",
        "this is a co-occurrence heuristic",
    );
    for needle in [
        "`40001`",
        "`P2034`",
        "does NOT retry",
        "bounded retry",
        "idempotent",
        "needs no retry path at all",
    ] {
        assert!(
            h[0].message.contains(needle),
            "money-tx-no-isolation-level: the landing lost {needle:?}: {}",
            h[0].message
        );
    }
}

// --- the counter-indication `multi-write-no-tx` was missing (2026-08-26) ---
//
// Its sibling `write-in-loop-no-tx` has carried this guard since 2026-08-25 ("THE REMEDY IS CONDITIONAL",
// "IF THE LOOP BODY TOUCHES NOTHING OUTSIDE THE DATABASE", both ahead of its own imperative); this rule
// shipped the bare imperative "Wrap both writes in `$transaction(...)`" with no occurrence of fetch, http,
// external, network, timeout, lock or connection anywhere in the message. The hazard is not hypothetical
// for it: an outside auditor read cal.com's `packages/app-store/larkcalendar/api/callback.ts` (byte-identical
// clone at `feishucalendar/`), where the two matched writes straddle an awaited HTTPS call to the calendar
// provider, and 4 of that tree's 27 findings are that shape — the other two are
// `packages/platform/examples/base/src/pages/api/{managed-user,oauth2-user}.ts`, both `prisma.user.create`
// -> `await fetch(...)` -> `prisma.user.update`.
//
// Following the imperative there converts the pair into an interactive transaction spanning a third-party
// round trip. Prisma's interactive default aborts at 5s (`@prisma/client` 6.16.1 in that tree, no
// `transactionOptions` anywhere in it), so a slow provider takes the transaction down and the credential
// write that commits today is rolled back with it — while the handler's catch logs and still redirects to
// the installed-apps page. The user sees a successful install that stored nothing.
//
// Unlike the loop rule, the timeout is claimed here because the call is DIRECTLY AWAITED at the site the
// clause was written for (`const primaryCalendarResponse = await fetch(...)`, callback.ts:93) rather than a
// fire-and-forget that resolves on the next microtask, which is why that same sentence was cut from the
// loop rule's message on 2026-08-25. The lock/connection half of the claim needs no latency premise at all.

#[test]
fn multi_write_message_puts_the_external_call_condition_before_the_transaction_imperative() {
    let dir = TempDir::new("zzop-db");
    // The OAuth-callback shape, reduced: create, call the provider, update — no transaction.
    dir.write(
        "src/callback.ts",
        "declare const prisma: any;\ndeclare function fetch(url: string, init?: any): Promise<any>;\nexport async function storeCredential(userId: string) {\n  const cred = await prisma.credential.create({ data: { userId } });\n  const primary = await fetch(\"https://provider.example.com/calendars/primary\");\n  await prisma.credential.update({ where: { id: cred.id }, data: { key: await primary.json() } });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "multi-write-no-tx");
    // The finding STILL FIRES, on the same line. This repair is a sentence repair: no matcher, veto,
    // severity or count moved, and a count that changes here means that contract was broken.
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);

    let m = &h[0].message;
    for needle in [
        // the negative branch — what a reader in the external-call case must NOT do
        "DO NOT WRAP THE PAIR",
        // the two mechanisms, in the order of how much they need to be true: the hold is
        // unconditional, the abort needs a slow response
        "row locks",
        "aborts at 5s",
        // the harm the abort produces — the write that is safe TODAY is the one that disappears
        "the row that commits today",
        // and the third leg §27 demands of any prescription: following it has to end green, so the
        // message must name what to do INSTEAD, not merely what not to do
        "Split the boundary instead",
    ] {
        assert!(
            m.contains(needle),
            "multi-write-no-tx's remedy no longer carries its counter-indication — missing {needle:?}. \
             A reader whose two writes straddle an HTTP call follows the transaction advice and ships an \
             install handler that shows success and stores nothing. In: {m}"
        );
    }

    // POSITION, not presence. A reader who acts on the first imperative never reaches a caveat behind it
    // (§27), so the pin asserts the offsets. INVALIDATION PROBE: move the conditioning clause behind the
    // imperative with every token above still present — `contains` stays green, this goes red.
    assert_disqualifier_summary_precedes_imperative(
        "multi-write-no-tx",
        m,
        "IF NOTHING BETWEEN THE TWO WRITES LEAVES THE DATABASE",
        "wrap both writes in `$transaction(...)`",
        "DO NOT WRAP THE PAIR",
    );
}
