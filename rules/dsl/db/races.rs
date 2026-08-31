//! `find-then-create-no-unique` + `non-atomic-counter-update` race tests (split from `db.rs`).

use super::*;

// --- find-then-create-no-unique ---

#[test]
fn find_first_then_create_with_no_unique_guard_is_flagged() {
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function ensureUserMarked(email: string) {\n  const existing = await prisma.user.findFirst({ where: { email } });\n  if (!existing) {\n    // zzop-find-then-create-no-unique-ok: low-traffic admin-only path, race window accepted\n    await prisma.user.create({ data: { email } });\n  }\n}\n",
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function incrementViewsMarked(id: string) {\n  const post = await prisma.post.findUnique({ where: { id } });\n  // zzop-non-atomic-counter-update-ok: single-writer batch job, no concurrent access possible\n  await prisma.post.update({ where: { id }, data: { views: post.views + 1 } });\n}\n",
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const rows: { key: string }[];\nexport async function ensureAllMarked() {\n  for (const r of rows) {\n    const e = await prisma.item.findFirst({ where: { key: r.key } });\n    if (!e) {\n      // zzop-check-then-act-in-loop-ok: single-threaded seed script, no concurrent workers\n      await prisma.item.create({ data: r });\n    }\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "check-then-act-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- idempotency-key-regenerated-in-loop ---

#[test]
fn idempotency_key_regenerated_via_random_uuid_inside_retry_loop_is_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/payments.ts",
        "declare const attempts: number[];\ndeclare const body: any;\ndeclare const api: any;\ndeclare function randomUUID(): string;\nexport async function chargeWithRetries() {\n  for (const attempt of attempts) {\n    const idempotencyKey = randomUUID();\n    await api.post(\"/charge\", body, { headers: { \"Idempotency-Key\": idempotencyKey } });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "idempotency-key-regenerated-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 7);
}

/// FP-adversarial (nearest harmless lookalike): the key is still generated with `randomUUID()`, but the
/// assignment sits BEFORE the loop and is reused across every attempt — the assignment's own line never
/// falls inside the loop span, so `trigger_in_loop` never satisfies.
#[test]
fn idempotency_key_generated_once_before_loop_and_reused_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/payments.ts",
        "declare const attempts: number[];\ndeclare const body: any;\ndeclare const api: any;\ndeclare function randomUUID(): string;\nexport async function chargeOnceKey() {\n  const idempotencyKey = randomUUID();\n  for (const attempt of attempts) {\n    await api.post(\"/charge\", body, { headers: { \"Idempotency-Key\": idempotencyKey } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "idempotency-key-regenerated-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn idempotency_key_derived_deterministically_inside_loop_is_not_flagged() {
    // `hash(o.id)` is not one of the recognized random-generator calls, so the trigger pattern never
    // matches this line at all.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/payments.ts",
        "declare const orders: { id: string }[];\ndeclare const api: any;\ndeclare function hash(id: string): string;\nexport async function chargeOrders() {\n  for (const o of orders) {\n    const idempotencyKey = hash(o.id);\n    await api.post(\"/charge\", o, { headers: { \"Idempotency-Key\": idempotencyKey } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "idempotency-key-regenerated-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn idempotency_regen_ok_marker_directly_above_the_regenerated_key_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/payments.ts",
        "declare const attempts: number[];\ndeclare const body: any;\ndeclare const api: any;\ndeclare function randomUUID(): string;\nexport async function chargeWithRetriesMarked() {\n  for (const attempt of attempts) {\n    // zzop-idempotency-key-regenerated-in-loop-ok: sandbox test harness, retries treated as new charges intentionally\n    const idempotencyKey = randomUUID();\n    await api.post(\"/charge\", body, { headers: { \"Idempotency-Key\": idempotencyKey } });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "idempotency-key-regenerated-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

// ORDER-GATE pin: seals the `after: read` gate — a `.create(` that PRECEDES the only read is not the
// check-then-act shape the id names ("find THEN create"). Before the gate, plain co-occurrence fired.
#[test]
fn a_create_that_precedes_the_only_find_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function createThenAudit(email: string) {\n  await prisma.user.create({ data: { email } });\n  const existing = await prisma.user.findFirst({ where: { email } });\n  return existing;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "find-then-create-no-unique").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- the HANDLED-COLLISION veto (2026-08-23) and the remedy sentence it comes with ---
//
// The two cal.com signup handlers (apps/web/app/api/auth/signup/handlers/calcomSignupHandler.ts:253
// and selfHostedHandler.ts:163) wear this exact shape: the flagged create is wrapped in a catch that
// tests Prisma's P2002 and returns 409. That IS the constraint-backed remedy this rule asks for, so
// the finding was a report against code that had already done the work — and the shipped remedy,
// read literally, named the one edit that breaks it (an `upsert` keyed on email overwrites the
// existing account's password hash).

#[test]
fn find_then_create_whose_create_is_wrapped_in_a_p2002_handler_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/signup.ts",
        "declare const prisma: any;\ndeclare function isPrismaError(e: any): boolean;\nexport async function signup(email: string, teamId: string) {\n  const team = await prisma.team.findUnique({ where: { id: teamId } });\n  try {\n    await prisma.user.create({ data: { email, teamId: team.id } });\n  } catch (error) {\n    if (isPrismaError(error) && error.code === \"P2002\") {\n      return { status: 409 };\n    }\n    throw error;\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "find-then-create-no-unique").is_empty(),
        "a handled unique violation IS the remedy this rule asks for: {:?}",
        out.findings
    );
}

/// CONTROL for the veto above, and the reason it is a near-twin rather than a fresh fixture: every
/// byte is the same except the error code, which names a CONNECTION failure instead of a unique
/// violation. Nothing about the try/catch, the team read or the user create closes the race, so the
/// finding must survive — if this goes quiet the veto is matching on the shape of a catch block
/// rather than on the vendor code it was written to read.
#[test]
fn find_then_create_whose_catch_names_no_unique_violation_still_fires() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/signup.ts",
        "declare const prisma: any;\ndeclare function isPrismaError(e: any): boolean;\nexport async function signup(email: string, teamId: string) {\n  const team = await prisma.team.findUnique({ where: { id: teamId } });\n  try {\n    await prisma.user.create({ data: { email, teamId: team.id } });\n  } catch (error) {\n    if (isPrismaError(error) && error.code === \"P1001\") {\n      return { status: 503 };\n    }\n    throw error;\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "find-then-create-no-unique");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

/// The remedy sentence, pinned AS DELIVERED rather than as authored: a competent outsider reading
/// this finding must be counter-indicated against the swap that breaks the code. Before 2026-08-23
/// the message said only "Add a unique constraint and replace the check-then-act with an
/// upsert/`connectOrCreate`" — an unconditional imperative whose obvious edit overwrites an
/// existing account's credential row.
#[test]
fn the_find_then_create_finding_counter_indicates_the_blind_upsert_swap() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function ensureUser(email: string) {\n  const existing = await prisma.user.findFirst({ where: { email } });\n  if (!existing) {\n    await prisma.user.create({ data: { email } });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "find-then-create-no-unique");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;
    for needle in [
        "KEEP THE INSERT and handle the unique violation",
        "correct ONLY when OVERWRITING the existing row is what you want",
        "hands the second caller the first caller's account",
        "where the answer is a credential take (1)",
    ] {
        assert!(
            m.contains(needle),
            "the delivered remedy no longer counter-indicates the overwrite edit -- missing {needle:?} in: {m}"
        );
    }
}

/// The remedy's ROOT step -- "make the looked-up columns unique" -- is a prescription whose
/// precondition this rule cannot check, and where the precondition is false the edit does not merely
/// fail to help, it takes the project down twice: the migration is REFUSED by any table that already
/// holds a duplicate of that pair, and forced through it rejects the second legitimate row and kills
/// whatever creates it. That shape is ordinary, not exotic -- one owner holding two linked accounts
/// with the same external provider, two installs of one integration under a tenant -- and until
/// 2026-08-25 nothing in this message asked the reader whether their pair was unique at all. Every
/// caveat it carried was about WHICH of the two ways of living with the constraint to take, so a
/// reader whose finding is TRUE (the race is real, the constraint is absent) is walked straight into
/// the breaking edit with the message agreeing all the way down.
///
/// Note that the two branches this message already spells out BOTH presuppose the constraint exists
/// -- (1) handles the violation "the constraint now raises", (2) swaps in an `upsert` that needs a
/// unique key to conflict on -- so closing them one at a time could never close this: the defect is
/// in the ROOT they both descend from, which is why the counter-indication attaches there.
///
/// TWO assertions, and the second is the one that matters (§27). Existence is not enough: a reader
/// who acts on the first instruction never reaches a caveat printed after it, so the pin asserts
/// POSITION -- `index(counter-indication) < index(imperative)` -- and an invalidation probe that
/// moves the clause behind the imperative turns this red with every token still present.
///
/// Deliberately target-independent: the shape is named by ROLE (a provider, an integration install,
/// an owner), never by any vendor, path, schema or corpus tree. The finding that motivated it is one
/// route in one measured repo and a pin that quoted it would pass for the wrong reason.
#[test]
fn the_find_then_create_remedy_questions_the_column_pair_before_it_prescribes_the_constraint() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function ensureUser(email: string) {\n  const existing = await prisma.user.findFirst({ where: { email } });\n  if (!existing) {\n    await prisma.user.create({ data: { email } });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "find-then-create-no-unique");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;

    assert_clauses_precede_imperative(
        "find-then-create-no-unique",
        m,
        "THE REMEDY IS THE UNIQUE CONSTRAINT",
        &[
            "NOT MEANT TO BE UNIQUE",
            "legitimately repeats",
            "migration FAILS OUTRIGHT",
            "second legitimate row is rejected",
            "distinguishes the two rows",
            "FOR UPDATE",
        ],
    );
}

// --- the STRING-DATA hole in the handled-collision veto, closed 2026-08-23 ---
//
// The veto arm shipped as a bare token alternation (`\bP2002\b|\b23505\b|...`) evaluated over the
// WHOLE function span with `strip_string_literals` off, so any occurrence of the token silenced the
// finding -- including one inside a string literal that is data, not a handler. Reproduced on a
// three-file probe tree before the narrowing: a `logger.info("... 23505 ...")` line and a docs URL
// ending `#P2002` each erased an unfixed check-then-act race, while the byte-identical file with the
// token changed to 99999 fired. `strip_string_literals: true` is NOT the fix and was checked first:
// this same rule's `safe` arm matches `ON CONFLICT`/`ON DUPLICATE KEY`, tokens that only ever appear
// INSIDE raw-SQL string literals, so flipping the matcher-wide flag would break that arm. The fix is
// per-arm: `handled` now requires the COMPARISON shape (`=== "P2002"`, `case "23505":`,
// `instanceof UniqueConstraintError`), which no bare mention can wear.

#[test]
fn find_then_create_whose_only_p2002_is_a_docs_url_in_a_string_still_fires() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/signup.ts",
        "declare const prisma: any;\nexport async function signup(email: string, teamId: string) {\n  const team = await prisma.team.findUnique({ where: { id: teamId } });\n  await prisma.user.create({ data: { email, teamId: team.id } });\n  return { docs: \"see https://prisma.io/docs/errors#P2002 for constraint errors\" };\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "find-then-create-no-unique");
    assert_eq!(
        h.len(),
        1,
        "a docs URL naming the error code is data, not a handler: {:?}",
        out.findings
    );
    assert_eq!(h[0].line, 4);
}

#[test]
fn find_then_create_whose_only_23505_is_a_log_message_still_fires() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/signup.ts",
        "declare const prisma: any;\ndeclare const logger: any;\nexport async function signup(email: string, teamId: string) {\n  const team = await prisma.team.findUnique({ where: { id: teamId } });\n  await prisma.user.create({ data: { email, teamId: team.id } });\n  logger.info(\"note: unrelated path can raise 23505 downstream\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "find-then-create-no-unique");
    assert_eq!(
        h.len(),
        1,
        "a log line naming the error code is data, not a handler: {:?}",
        out.findings
    );
    assert_eq!(h[0].line, 5);
}

/// CONTROL for the two fixtures above, and the reason they are near-twins of it rather than fresh
/// files: byte-identical to the log-line probe except that the token is 99999, a code no vendor uses
/// and this arm has never contained. It fired before the narrowing and fires after it, so if it ever
/// goes quiet the cause is the TRIGGER shape (the find/create pair) collapsing, not the veto -- which
/// is the one confound that would make the two probes above pass for the wrong reason.
#[test]
fn find_then_create_whose_log_message_names_no_vendor_code_fires_as_the_control() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/signup.ts",
        "declare const prisma: any;\ndeclare const logger: any;\nexport async function signup(email: string, teamId: string) {\n  const team = await prisma.team.findUnique({ where: { id: teamId } });\n  await prisma.user.create({ data: { email, teamId: team.id } });\n  logger.info(\"note: unrelated path can raise 99999 downstream\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "find-then-create-no-unique");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

/// The delivered message must DESCRIBE the narrowed arm, not the one it replaced: a shipped sentence
/// calling this an error-code "test" while the matcher accepted a bare mention is exactly the prose
/// overclaim that made the hole invisible on inspection.
#[test]
fn the_find_then_create_message_states_that_a_bare_mention_is_not_a_handler() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function ensureUser(email: string) {\n  const existing = await prisma.user.findFirst({ where: { email } });\n  if (!existing) {\n    await prisma.user.create({ data: { email } });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "find-then-create-no-unique");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;
    for needle in [
        "a COMPARISON against the vendor's collision code, not a mention of it",
        "A BARE TOKEN DOES NOT SILENCE THIS RULE",
        "EVIDENCE the collision is handled, never proof",
    ] {
        assert!(
            m.contains(needle),
            "the delivered message no longer describes the arm it ships with -- missing {needle:?} in: {m}"
        );
    }
}

// --- the same veto on the loop sibling (FINDING 4, 2026-08-23) ---
//
// `check-then-act-in-loop` names the same race and carried the same `safe` vocabulary, but got no
// `handled` arm when the sibling did. A function that had genuinely closed the collision therefore
// went silent under `find-then-create-no-unique` and still fired here -- and this rule's message
// delegates its remedy to the sibling's, which was absent from that run's output.

#[test]
fn loop_create_whose_collision_is_handled_by_a_p2002_test_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/import.ts",
        "declare const prisma: any;\nexport async function importAll(emails: string[]) {\n  const existing = await prisma.user.findFirst({ where: { email: emails[0] } });\n  for (const email of emails) {\n    try {\n      await prisma.user.create({ data: { email } });\n    } catch (e: any) {\n      if (e.code === \"P2002\") continue;\n      throw e;\n    }\n  }\n  return existing;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "check-then-act-in-loop").is_empty(),
        "a handled unique violation IS the remedy this rule asks for: {:?}",
        out.findings
    );
}

/// CONTROL for the veto above: every byte the same except the code, which names a CONNECTION failure.
/// Nothing closes the race, so the loop finding must survive.
#[test]
fn loop_create_whose_catch_names_no_unique_violation_still_fires() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/import.ts",
        "declare const prisma: any;\nexport async function importAll(emails: string[]) {\n  const existing = await prisma.user.findFirst({ where: { email: emails[0] } });\n  for (const email of emails) {\n    try {\n      await prisma.user.create({ data: { email } });\n    } catch (e: any) {\n      if (e.code === \"P1001\") continue;\n      throw e;\n    }\n  }\n  return existing;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "check-then-act-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

/// The loop sibling DELEGATES the long form of the remedy to `find-then-create-no-unique`'s message,
/// and a reader who has only this finding in front of them never opens that one — so the delegation
/// could not carry the counter-indication for it. Its own imperative ("add a unique constraint") has
/// the same precondition the family's owner now states: a pair one owner may legitimately hold twice
/// (two linked accounts at the same external provider, two installs of one integration) takes no such
/// constraint, and prescribing it refuses the migration and then rejects the second legitimate row.
///
/// Asserts POSITION, not presence (§27); named by ROLE only — no vendor, path, schema or corpus tree.
#[test]
fn the_loop_sibling_remedy_questions_the_column_pair_before_it_prescribes_the_constraint() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/seed.ts",
        "declare const prisma: any;\ndeclare const rows: { key: string }[];\nexport async function ensureAllMarked() {\n  for (const r of rows) {\n    const e = await prisma.item.findFirst({ where: { key: r.key } });\n    if (!e) {\n      await prisma.item.create({ data: r });\n    }\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "check-then-act-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;

    assert_clauses_precede_imperative(
        "check-then-act-in-loop",
        m,
        "add a unique constraint",
        &[
            "NOT MEANT TO BE UNIQUE",
            "legitimately hold twice",
            "migration FAILS OUTRIGHT",
            "second legitimate row is rejected",
        ],
    );
}
