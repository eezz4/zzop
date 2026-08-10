//! `update-delete-no-where` + `unawaited-write` tests (split from `db.rs`; shared fixtures live in the crate root).

use super::*;

// --- update-delete-no-where ---

#[test]
fn update_many_with_no_where_anywhere_in_function_is_flagged() {
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function bulkArchiveMarked() {\n  // zzop-update-delete-no-where-ok: admin console confirmed intentional full-table archive\n  await prisma.order.updateMany({ data: { archived: true } });\n}\n",
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
fn catch_chained_write_call_is_not_flagged() {
    // The message's remedy list promises attaching `.then()`/`.catch()` clears. The sibling
    // `unawaited-transaction` has carried the `\.catch\b` exclude arm since its v0.21.0 repair;
    // this rule's matcher drifted without it, so `.create(...).catch(named)` — a handled promise —
    // still fired. The fire-and-forget control right above stays flagged.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function logEventCaught(id: string) {\n  prisma.event.create({ data: { id } }).catch(reportWriteFailure);\n}\ndeclare function reportWriteFailure(e: unknown): void;\n",
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
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function logEventMarked(id: string) {\n  // zzop-unawaited-write-ok: best-effort audit log, failure intentionally ignored\n  prisma.event.create({ data: { id } });\n}\n",
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
    let dir = TempDir::new("zzop-db");
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
fn multi_line_concise_arrow_body_returning_the_write_is_not_flagged() {
    // Confirmed FP (2026-08-09): a formatter-wrapped concise arrow body DOES return the promise —
    // callers await it — but the `=>` sits at the end of the PREVIOUS line, where a same-line
    // exclusion cannot see it. The one-line-lookback exclusion must keep this silent. The must-fire
    // control for this shape is `fire_and_forget_create_call_is_flagged` above.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ntype Order = { id: string };\nconst persistOrder = (o: Order) =>\n  prisma.order.create({ data: o });\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-write").is_empty(),
        "a wrapped concise arrow body returns the promise: {:?}",
        out.findings
    );
}

#[test]
fn multi_line_assignment_continuation_write_is_not_flagged() {
    // Same continuation class, assignment flavor: `const p =` at end of line, the write on the next
    // line. The promise is captured (and returned below), not fire-and-forget.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport function persist(id: string) {\n  const p =\n    prisma.event.create({ data: { id } });\n  return p;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-write").is_empty(),
        "an assignment continuation captures the promise: {:?}",
        out.findings
    );
}

#[test]
fn fire_and_forget_write_after_a_complete_previous_statement_is_still_flagged() {
    // The lookback must veto only CONTINUATION shapes (`=>`/`=` ending the previous line). A previous
    // line that is a complete statement — even one containing `return` — leaves a bare write on the
    // next line genuinely fire-and-forget.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport function track(id: string, skip: boolean) {\n  if (skip) return;\n  prisma.event.create({ data: { id } });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-write");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn awaited_prisma_user_create_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
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
