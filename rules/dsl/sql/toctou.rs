use crate::{assert_clauses_precede_imperative, hits, scan, TempDir};

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
    // The finding's line is the WRITE call's line (7), not the read declaration's (3) — `trigger` is
    // `write` gated by `after: read`, so the reported line marks the racing action, and it is
    // specifically the first write that LEXICALLY FOLLOWS a read. The `.delete(` on line 5 is not in
    // the `write` pattern's create/upsert/insert set, so line 7 is the first candidate either way.
    assert_eq!(hits[0].line, 7, "{:?}", out.findings);
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

/// The 2026-08-03 anchor alignment: the `routes/` and `controllers/` arms used to spell `.*/` — at
/// least one directory ABOVE them — while the `api/` arm was `(?:^|/)`, so this exact fixture (a
/// TOP-LEVEL `routes/` directory, the layout `express-generator` scaffolds) was silently out of scope.
/// All three directory arms now share the `(?:^|/)` idiom, the same verdict `sql/nplus1`'s root anchor
/// received. The old spelling structurally cannot match a path with no `/` before `routes/`, so this
/// fixture asserts the alignment's entire gain: reverting it takes this from 1 finding to 0.
#[test]
fn a_toggle_in_a_top_level_routes_directory_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "routes/likes.ts",
        "declare const likeStore: any;\nexport async function toggle() {\n  const existing = await likeStore.findOne((l: any) => l.id === \"x\");\n  if (!existing) {\n    await likeStore.create({ id: \"y\" });\n  }\n}\n",
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
    // fix (matching the db sibling `find-then-create-no-unique` correction), so this fixture,
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
fn toctou_ok_marker_directly_above_the_write_line_suppresses_the_finding() {
    // Suppression is anchored on the TRIGGER line (`method_scan`'s `marker_suppresses`), and `trigger`
    // is now `write` — so the marker belongs directly above the `.create(` call, not above the read.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createMarkedHandlers.ts",
        "declare const likeStore: any;\nexport async function toggle() {\n  const existing = await likeStore.findOne((l: any) => l.id === \"x\");\n  if (existing) {\n    await likeStore.delete(existing.id);\n  } else {\n    // zzop-race-condition-toctou-ok: intentional single-writer admin path\n    await likeStore.create({ id: \"y\" });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "race-condition-toctou").is_empty(),
        "{:?}",
        out.findings
    );
}

// ORDER-GATE pin: seals the `after: read` gate's whole contribution — a write that PRECEDES the only
// read is not a check-then-act race, and the id ("time-of-check-time-of-use") asserts that order. Before
// the gate this fired on the read line; if the gate is ever removed, this goes red instead of the
// message quietly becoming false again.
#[test]
fn a_write_that_precedes_the_only_read_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createAuditHandlers.ts",
        "declare const auditStore: any;\ndeclare const likeStore: any;\nexport async function logThenLookUp() {\n  await auditStore.create({ id: \"a\" });\n  const existing = await likeStore.findOne((l: any) => l.id === \"x\");\n  return existing;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "race-condition-toctou").is_empty(),
        "{:?}",
        out.findings
    );
}

/// FINDING 1 (2026-08-23): this rule still shipped the unconditional imperative the batch that
/// rewrote `db/find-then-create-no-unique` existed to remove -- "Add a unique constraint and replace
/// the check-then-act with an upsert", with no overwrite counter-indication and no pointer to one.
/// It is reachable on its own (`--rule sql/race-condition-toctou`, `packs.only: ["sql"]`), so the
/// sibling message does not rescue the reader, and it is harmful: on cal.com the rule reports
/// `addSecondaryEmail.handler.ts:45`, which reads a secondary email by address and rejects it as
/// already taken -- following the old remedy literally reassigns another user's verified address to
/// the caller. Pinned AS DELIVERED, because "the message says it" is the only property that matters.
#[test]
fn the_toctou_finding_counter_indicates_the_blind_upsert_swap() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createSubHandlers.ts",
        "declare const subStore: any;\nexport async function subscribe() {\n  const existing = await subStore.findOne((s: any) => s.id === \"x\");\n  if (!existing) {\n    await subStore.create({ id: \"y\" });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "race-condition-toctou");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;
    for needle in [
        "KEEP THE WRITE and handle the unique violation",
        "correct ONLY when OVERWRITING the existing row is what you want",
        "hands the second caller the first caller's record",
        "where the answer is a credential or an ownership column, take (1)",
    ] {
        assert!(
            m.contains(needle),
            "the delivered remedy no longer counter-indicates the overwrite edit -- missing {needle:?} in: {m}"
        );
    }
    assert!(
        !m.contains("replace the check-then-act with an upsert"),
        "the unconditional imperative is back: {m}"
    );
}

/// The ROOT of this remedy — "add a unique constraint" — is a prescription whose precondition this
/// rule cannot check, and this message used to assert the precondition away in so many words ("that
/// part is unconditional"). It is not unconditional: where the looked-up pair is one an owner may
/// legitimately hold twice (two linked accounts at the same external provider, two installs of one
/// integration), the constraint takes the project down twice over — the migration is refused by any
/// table already holding such a duplicate, and forced through it rejects the second legitimate row.
/// Both of this message's two named ways of "living with it" presuppose the constraint EXISTS, so
/// neither could ever counter-indicate the root they descend from; the clause attaches to the root.
///
/// Asserts POSITION, not presence (§27): a reader who acts on the first instruction never reaches a
/// caveat printed after it. Named by ROLE only — no vendor, path, schema or corpus tree.
#[test]
fn the_toctou_remedy_questions_the_column_pair_before_it_prescribes_the_constraint() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createSubHandlers.ts",
        "declare const subStore: any;\nexport async function subscribe() {\n  const existing = await subStore.findOne((s: any) => s.id === \"x\");\n  if (!existing) {\n    await subStore.create({ id: \"y\" });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "race-condition-toctou");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;

    assert!(
        !m.contains("that part is unconditional"),
        "the message asserts away the very precondition it cannot check: {m}"
    );

    assert_clauses_precede_imperative(
        "race-condition-toctou",
        m,
        "add a unique constraint",
        &[
            "NOT MEANT TO BE UNIQUE",
            "legitimately hold twice",
            "migration FAILS OUTRIGHT",
            "second legitimate row is rejected",
            "distinguishes the two rows",
        ],
    );
}

// --- the RECEIVER this trigger never reads (2026-09-11) ---
//
// This rule's write arm is the widest of the three in the family: `\b(?:create|upsert|insert)\s*\(`
// requires no receiver at all, so a free `create(row)` is read as a database write. The measured
// receiver census lives on `db/find-then-create-no-unique` (cal.com, 38 findings, 2026-09-11) and that
// rule's message owns it; this pack declines the same single receiver — the literal `this.<verb>(` —
// and discloses the rest rather than guessing at a vendor list.

/// The veto, with anti-vacuity: a real store write in the same controller still fires, so the silence
/// at the this-receiver line is a decline and not a dead rule.
///
/// INVALIDATION: delete `trigger_call_exclude_pattern` from `race-condition-toctou` and this reports 2
/// findings with the first at line 6.
#[test]
fn a_write_on_the_enclosing_classes_own_this_receiver_is_declined_here_too() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/subHandlers.ts",
        "declare const subStore: any;\nexport class SubController {\n  async viaOwnMethod() {\n    const existing = await subStore.findOne((s: any) => s.id === \"x\");\n    if (!existing) {\n      await this.create({ id: \"y\" });\n    }\n  }\n  async viaStore() {\n    const existing = await subStore.findOne((s: any) => s.id === \"x\");\n    if (!existing) {\n      await subStore.create({ id: \"y\" });\n    }\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "race-condition-toctou");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(
        h[0].line, 12,
        "the anchor must be the store write, not the this-receiver call at line 6: {:?}",
        out.findings
    );
}

/// POSITION pin (rule-quality.md 27) for the half that stays disclosed: the bare-word write arm is
/// named ahead of the unique-constraint remedy, and it points at the sibling that carries the census.
#[test]
fn the_toctou_message_says_the_write_arm_needs_no_receiver_before_it_prescribes_the_constraint() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/ensureHandlers.ts",
        "declare const store: any;\nexport async function ensureRow(id: string) {\n  const existing = await store.findOne((s: any) => s.id === id);\n  if (!existing) {\n    await store.create({ id });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "race-condition-toctou");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;
    for needle in [
        "THIS TRIGGER NEVER READS IT",
        "does not even require one to exist",
        "db/find-then-create-no-unique",
    ] {
        assert!(
            m.contains(needle),
            "race-condition-toctou lost its receiver clause — missing {needle:?}. In: {m}"
        );
    }
    assert_clauses_precede_imperative(
        "race-condition-toctou",
        m,
        "Where the pair IS unique, add a unique constraint",
        &["FIRST CHECK THE RECEIVER", "no row and no table"],
    );
}
