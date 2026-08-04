use crate::{hits, scan, TempDir};

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
