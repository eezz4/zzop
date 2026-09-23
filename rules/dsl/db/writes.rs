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
fn delete_many_handed_its_whole_argument_by_name_is_not_flagged() {
    // The rule's OWN message named this shape as a measured over-report — "the whole-argument form
    // (`updateMany(args)`) ... fires at `critical` on a scoped write" — and until now the prediction was
    // not wired into the judgment. `deleteMany(toDelete)` builds its argument object somewhere this
    // matcher never reads, so the absence of a `where:` token here is a statement about THIS function,
    // not about the query. Blind-corpus subject: cal.com
    // `packages/features/calendar-subscription/lib/cache/CalendarCacheEventService.ts:66` fired
    // `critical` on `calendarCacheEventRepository.deleteMany(toDelete)`, whose repository runs
    // `prisma.calendarCacheEvent.deleteMany({ where: { OR: conditions } })` with an empty-array early
    // return. Same class as the arrow-predicate carve-outs above, generalized from "the first argument
    // is a function" to "the first argument is not an object literal this matcher can read".
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const calendarCacheEventRepository: any;\nexport async function pruneCache(toDelete: any[]) {\n  await calendarCacheEventRepository.deleteMany(toDelete);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn update_many_whose_arguments_open_on_the_next_line_is_not_flagged() {
    // The multi-line spelling of the same shape, and the one the corpus actually carries: seven of the
    // nine corpus findings were `mongoQueryRunner.updateMany(` / `.deleteMany(` with the filter argument
    // on the FOLLOWING line (typeorm `src/entity-manager/MongoEntityManager.ts`,
    // `src/driver/mongodb/MongoQueryRunner.ts`). A trigger that accepted an open paren at end of line
    // would keep every one of them.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/runner.ts",
        "declare const collection: any;\nexport async function applyAll(filter: any, update: any) {\n  return collection.updateMany(\n    filter,\n    update,\n  );\n}\n",
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
fn delete_many_handed_an_empty_argument_object_inline_is_still_flagged() {
    // The PLANTED canary for the narrowed trigger, and it is planted because it has to be: after the
    // narrowing, the surviving positive shape has ZERO instances across the whole corpus (every one of
    // the nine findings the corpus carried was a call handed its arguments by name), so no corpus
    // measurement can price what was kept. This fixture is one character away from
    // `delete_many_handed_its_whole_argument_by_name_is_not_flagged` above — `({})` versus `(toDelete)`
    // — which is exactly the discriminator the trigger now draws.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function wipeAll() {\n  await prisma.order.deleteMany({});\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "update-delete-no-where");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
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

// --- unawaited-write: the UPWARD enclosing-call veto (`enclosing_call_exclude_pattern`) ---
//
// Prisma's ARRAY transaction takes UN-awaited query builders as its elements: awaiting inside the
// array literal runs each query immediately and OUTSIDE the transaction. The rule's remedy ("await
// the call, or return it") therefore BREAKS the code at every such element, which is why the veto had
// to reach the `$transaction([` opener several lines above rather than sharpen the same-line regex.

#[test]
fn array_transaction_elements_are_not_flagged_across_a_long_element_body() {
    // R1 — the shape the veto exists for: two un-awaited builders inside one `$transaction([...])`,
    // the second one 18 lines below the opener. BOTH must go quiet; awaiting either would take it out
    // of the transaction.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        r#"declare const prisma: any;
export async function moveBooking(id: string, target: string) {
  const [moved, logged] = await prisma.$transaction([
    prisma.booking.update({
      where: { uid: id },
      data: {
        f1: target,
        f2: target,
        f3: target,
        f4: target,
        f5: target,
        f6: target,
        f7: target,
        f8: target,
        f9: target,
        f10: target,
        f11: target,
        f12: target,
      },
    }),
    prisma.auditLog.create({
      data: {
        g1: id,
        g2: id,
        g3: id,
        g4: id,
        g5: id,
        g6: id,
      },
    }),
  ]);
  return [moved, logged];
}
"#,
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-write").is_empty(),
        "both array-transaction elements must go quiet: {:?}",
        out.findings
    );
}

#[test]
fn array_transaction_element_twenty_three_lines_under_the_opener_is_not_flagged() {
    // R2 — the farthest opener->finding distance measured on cal.com @ `db/unawaited-write` is 23
    // lines, and the two leading elements are `deleteMany` calls the rule's own `line_pattern` never
    // matches (so a one-line lookback would never have reached the elements that DO fire).
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        r#"declare const prisma: any;
export async function replaceRows(id: string) {
  await prisma.$transaction([
    prisma.a.deleteMany({ where: { id } }),
    prisma.b.deleteMany({ where: { id } }),
    prisma.c.create({
      data: {
        h1: id,
        h2: id,
        h3: id,
        h4: id,
        h5: id,
        h6: id,
        h7: id,
        h8: id,
        h9: id,
        h10: id,
        h11: id,
        h12: id,
        h13: id,
        h14: id,
        h15: id,
        h16: id,
      },
    }),
    prisma.d.update({ where: { id }, data: { n: 1 } }),
  ]);
}
"#,
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-write").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn the_callback_form_of_a_transaction_still_flags_a_fire_and_forget_write() {
    // R3 — the must-fire control, and the ONLY thing holding it: inside the CALLBACK form the write
    // really is fire-and-forget and `await tx.user.create(...)` really is the fix. cal.com carries 16
    // callback-form sites and none of them fires today, so a veto that went body-scoped would be
    // invisible on the corpus and visible only here.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        r#"declare const prisma: any;
export async function register(data: unknown) {
  await prisma.$transaction(async (tx: any) => {
    tx.user.create({ data });
  });
}
"#,
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-write");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn a_write_after_a_closed_array_transaction_is_still_flagged() {
    // R5 — the walk must CONSUME a group it sees close (`]);`) rather than climbing out of one
    // statement into the previous one. The `$transaction([...])` above is already closed, so it can
    // say nothing about the bare write that follows it.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        r#"declare const prisma: any;
export async function settle(id: string) {
  await prisma.$transaction([
    prisma.a.create({ data: { id } }),
  ]);
  prisma.b.create({ data: { id } });
}
"#,
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-write");
    assert_eq!(
        h.len(),
        1,
        "the veto must not go body-scoped: {:?}",
        out.findings
    );
    assert_eq!(h[0].line, 6);
}

#[test]
fn awaited_promise_all_array_elements_are_not_flagged() {
    // R6 — same non-atomic-if-awaited shape without Prisma: `Promise.all([...])` consumes each
    // element's promise, and awaiting inside the array serializes the calls it exists to parallelize.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        r#"declare const prisma: any;
export async function seed(a: unknown, b: unknown) {
  await Promise.all([
    prisma.x.create({ data: a }),
    prisma.y.create({ data: b }),
  ]);
}
"#,
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unawaited-write").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_builder_pushed_onto_an_array_before_the_transaction_still_fires_and_the_message_says_so() {
    // R7 — the residual, pinned rather than hoped for: when the builder is collected by
    // `arr.push(...)` and the transaction later spreads it (`$transaction([...ops])`), the enclosing
    // call at the site is `push(`, which no enclosing-call veto can distinguish from a real
    // fire-and-forget. 1 of cal.com's 18 has this shape. The rule's own message must disclose it.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        r#"declare const prisma: any;
export async function bulk(ids: string[]) {
  const ops: any[] = [];
  ops.push(prisma.x.update({ where: { id: ids[0] }, data: { n: 1 } }));
  await prisma.$transaction(ops);
}
"#,
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-write");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
    assert!(
        h[0].message.contains("push("),
        "the message must disclose the out-of-reach shape: {}",
        h[0].message
    );
}

#[test]
fn an_unawaited_promise_all_array_still_flags_its_elements() {
    // R9 — the reason the veto pattern demands `await`/`return`/`yield` ON THE OPENER LINE: an
    // enclosing call whose OWN promise nobody consumes leaves the writes genuinely unawaited, so
    // vetoing on the opener text alone would hide a real bug.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        r#"declare const prisma: any;
export function seedLoose(a: unknown) {
  Promise.all([
    prisma.x.create({ data: a }),
  ]);
}
"#,
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-write");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

// --- §27 ordering pins (2026-08-26) ---
//
// Both rules in this file shipped a clause naming a measured shape on which their OWN finding is wrong,
// and both put it behind the remedy. The repair is presentation only — no matcher, veto, severity or
// count moved, and each pin asserts the unchanged count next to the offsets so a detection change cannot
// hide inside a message edit.

/// `update-delete-no-where` is the `critical` one, and the shape it disqualifies is the shape it is most
/// likely to be handed: a filter that IS there, spelled as a spread or an object shorthand, so no `where:`
/// token appears and the rule fires at `critical` anyway. That sentence used to sit ~490 bytes behind
/// "Add a `where:` filter". The remedy now carries the premise instead.
#[test]
fn update_delete_no_where_message_puts_the_scope_condition_before_the_where_imperative() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function bulkArchive() {\n  await prisma.order.updateMany({ data: { archived: true } });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "update-delete-no-where");
    // Detection is untouched: same site, same line, same severity band.
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    assert_disqualifier_summary_precedes_imperative(
        "update-delete-no-where",
        &h[0].message,
        "IF NO FILTER ALREADY SCOPES THIS CALL",
        "add a `where:` filter",
        "both fire at `critical` on a scoped write",
    );
}

/// `unawaited-write` already CONDITIONED its remedy — but the condition trailed the verb ("Await the call
/// ... — EXCEPT inside an array transaction"), so a reader who acts on the verb still acts first. The
/// premise now sits in front of it, which also puts it ahead of the residual this pin names: the veto keys
/// on the OPENER's own `await`, so a `$transaction([...])` bound to a variable and awaited on the next
/// line keeps firing — a finding the remedy must not be applied to.
#[test]
fn unawaited_write_message_puts_the_array_transaction_condition_before_the_await_imperative() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function recordAudit(actorId: string) {\n  prisma.audit.create({ data: { actorId } });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-write");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    assert_disqualifier_summary_precedes_imperative(
        "unawaited-write",
        &h[0].message,
        "IF THE CALL IS NOT AN ELEMENT OF AN ARRAY TRANSACTION",
        "await the call (or return it",
        "still fires, which is the safe direction",
    );
}

// --- the DEFERRED SPREAD: the shape that survived the veto, and the whole live population (2026-09-11) ---
//
// The 2026-08-21 blind audit recorded 18 findings on cal.com in three shapes — `$transaction([` 13,
// `Promise.all([` 4, `arr.push(` 1 — and said the third could not be reached by any enclosing-call
// veto. The upward veto has since landed and taken the first two: re-measured 2026-09-11 against
// cal.com at its pinned commit, `zzop analyze --rule db/unawaited-write` reports **1** finding in the
// whole tree, and it is the third shape —
// `packages/features/bookings/lib/handleSeats/reschedule/owner/combineTwoSeatedBookings.ts:62`,
// `moveAttendeeCalls.push(prisma.attendee.update({...}))` spread into
// `await prisma.$transaction([...moveAttendeeCalls, ...])` 22 lines below.
//
// So the disclosure is no longer one residual among three. It is what the rule says on every finding
// it currently produces in the measured corpus, and the remedy printed above it BREAKS that code: the
// element runs immediately and outside the transaction. R7 below pins that the shape still fires;
// these two pin that the reader is told, at the imperative, to look DOWN rather than only up.

/// The measured cal.com shape at full distance — the push and the spread 20+ lines apart, with the
/// array built in a loop, so the finding is not an artifact of a three-line fixture.
///
/// INVALIDATION: this is the FIRING direction, so the probe is the opposite one — give
/// `enclosing_call_exclude_pattern` a downward twin that matches `$transaction(` anywhere in the body
/// and this goes to 0 findings. That is precisely the veto the rule declines to build (it would waive
/// every fire-and-forget write in any function that happens to run a transaction later), which is why
/// the shape is DISCLOSED instead.
#[test]
fn a_builder_pushed_in_a_loop_and_spread_twenty_lines_later_still_fires() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/combine.ts",
        r#"declare const prisma: any;
declare const attendees: { id: string }[];
export async function combine(targetId: string) {
  const moveAttendeeCalls = [];
  for (const attendeeToMove of attendees) {
    moveAttendeeCalls.push(
      prisma.attendee.update({
        where: {
          id: attendeeToMove.id,
        },
        data: {
          bookingId: targetId,
          f1: targetId,
          f2: targetId,
          f3: targetId,
          f4: targetId,
          f5: targetId,
          f6: targetId,
          f7: targetId,
          f8: targetId,
        },
      })
    );
  }
  await prisma.$transaction([
    ...moveAttendeeCalls,
  ]);
}
"#,
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-write");
    assert_eq!(
        h.len(),
        1,
        "the deferred-spread element must still fire — it is the population this rule's disclosure \
         is written for: {:?}",
        out.findings
    );
    assert_eq!(h[0].line, 7, "{:?}", out.findings);
}

/// POSITION pin (rule-quality.md 27): the condition on the imperative has to reach FORWARD, because
/// the shape it disqualifies sits BELOW the reported line and the veto only walks up. A reader looking
/// at `ops.push(prisma.x.update(...))` sees no array transaction anywhere near the call, satisfies
/// "IF THE CALL IS NOT AN ELEMENT OF AN ARRAY TRANSACTION" in good faith, awaits it, and takes the
/// write out of the transaction — which is the exact failure the clause two sentences later describes.
#[test]
fn the_unawaited_write_condition_reaches_forward_to_the_deferred_spread_before_the_imperative() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/audit.ts",
        "declare const prisma: any;\nexport async function recordAuditForward(actorId: string) {\n  prisma.audit.create({ data: { actorId } });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "unawaited-write");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;

    for needle in [
        // the condition itself has to say "later", not only "here"
        "INCLUDING ONE IT ONLY REACHES FURTHER DOWN THE FUNCTION",
        // the mechanism, so the reader knows why the tool cannot see it
        "the transaction is not an enclosing call of this line at all",
        // the measurement that makes this the main case rather than a footnote
        "ONE finding in the whole tree and that finding is this shape",
        // and the actionable instruction that replaces the veto
        "search the REST of the function",
    ] {
        assert!(
            m.contains(needle),
            "unawaited-write lost its deferred-spread disclosure — missing {needle:?}. That shape is \
             this rule's only measured corpus population, and the remedy above it breaks the code \
             there. In: {m}"
        );
    }

    // INVALIDATION PROBE: move the forward-reaching half of the condition behind the verb with every
    // token above still present — every contains assertion stays green and this call goes red.
    assert_disqualifier_summary_precedes_imperative(
        "unawaited-write",
        m,
        "INCLUDING ONE IT ONLY REACHES FURTHER DOWN THE FUNCTION",
        "await the call (or return it",
        "the transaction is not an enclosing call of this line at all",
    );
}
