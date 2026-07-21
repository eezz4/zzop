//! `update-delete-no-where` + `unawaited-write` tests (split from `be-db.rs`; shared fixtures live in the crate root).

use super::*;

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
fn delete_many_with_an_empty_where_object_is_flagged() {
    // `where: {}` is an EMPTY filter — Prisma treats it as no filter and deletes/updates every row,
    // exactly the whole-table write this rule exists to catch. It must NOT be vetoed by the presence of
    // the `where:` token alone.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function wipe() {\n  await prisma.order.deleteMany({ where: {} });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "update-delete-no-where");
    assert_eq!(
        h.len(),
        1,
        "empty `where: {{}}` must still flag: {:?}",
        out.findings
    );
    assert_eq!(h[0].line, 3);
}

#[test]
fn delete_many_with_a_multiline_populated_where_is_not_flagged() {
    // A real `where` object opened at end of line (multi-line) must still veto — the empty-object carve-out
    // treats `where: {` at EOL as populated (the empty multi-line `where: {\n}` shape is not a real idiom).
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function archiveOld() {\n  await prisma.order.deleteMany({\n    where: {\n      archived: false,\n    },\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-delete-no-where").is_empty(),
        "a populated multi-line where must not be flagged: {:?}",
        out.findings
    );
}

#[test]
fn delete_many_with_a_where_key_split_from_its_value_across_lines_is_not_flagged() {
    // A bare `where:` at end of line with the filter object opening on the NEXT line (a real, if
    // non-Prettier, formatting) must still veto: the per-line matcher can't see the next line, so the
    // `where:`-at-EOL alternative treats it as populated. Guards against the widened empty-object regex
    // re-introducing a false positive on genuinely-filtered multi-line deletes.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function archiveOld() {\n  await prisma.order.deleteMany({\n    where:\n      { archived: false },\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-delete-no-where").is_empty(),
        "a where-key split across lines must not be flagged: {:?}",
        out.findings
    );
}

#[test]
fn delete_many_with_a_dynamic_where_variable_is_not_flagged() {
    // `where: filter` (a computed filter object) is a real filter — veto, don't flag.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function archive(filter: any) {\n  await prisma.order.deleteMany({ where: filter });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-delete-no-where").is_empty(),
        "a dynamic `where: var` must not be flagged: {:?}",
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
fn unawaited_write_with_a_comparison_in_the_payload_is_still_flagged() {
    // Regression pin: the assignment-veto must not be tripped by a comparison operator (`>=`, `===`)
    // inside the write's payload. A bare unawaited `update` whose data contains `score >= threshold` is
    // still fire-and-forget and must flag — the former `=\s*\w` veto wrongly matched the `= t` in `>= t`.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function grade(id: string, score: number, threshold: number) {\n  prisma.user.update({ where: { id }, data: { verified: score >= threshold } });\n}\n",
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
