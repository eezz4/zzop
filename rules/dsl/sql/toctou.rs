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
