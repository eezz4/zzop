//! `pagination-no-orderby` + `unbounded-user-limit` + comment-skip/test-path exclusion tests (split from `db.rs`).

use super::*;

// --- pagination-no-orderby ---

#[test]
fn skip_take_pagination_with_no_orderby_is_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function listUsers(page: number) {\n  return prisma.user.findMany({ skip: page * 20, take: 20 });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "pagination-no-orderby");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn skip_take_pagination_with_orderby_in_same_function_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function listUsersSorted(page: number) {\n  return prisma.user.findMany({ skip: page * 20, take: 20, orderBy: { id: \"asc\" } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "pagination-no-orderby").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn skip_take_pagination_with_an_empty_orderby_object_is_flagged() {
    // `orderBy: {}` is an EMPTY sort spec — Prisma applies no ordering, so pagination is just as
    // unstable as with no `orderBy` at all. The empty-object carve-out (mirroring update-delete-no-where)
    // means the bare `orderBy:` token must NOT veto when its value is `{}`.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function listUsers(page: number) {\n  return prisma.user.findMany({ skip: page * 20, take: 20, orderBy: {} });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "pagination-no-orderby");
    assert_eq!(
        h.len(),
        1,
        "empty `orderBy: {{}}` must still flag: {:?}",
        out.findings
    );
    assert_eq!(h[0].line, 3);
}

#[test]
fn skip_take_pagination_with_a_dynamic_orderby_variable_is_not_flagged() {
    // `orderBy: sortSpec` (a computed sort object) is a real ordering — veto, don't flag.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function listUsers(page: number, sortSpec: any) {\n  return prisma.user.findMany({ skip: page * 20, take: 20, orderBy: sortSpec });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "pagination-no-orderby").is_empty(),
        "a dynamic `orderBy: var` must not be flagged: {:?}",
        out.findings
    );
}

#[test]
fn comment_mentioning_the_word_skip_with_a_colon_is_not_flagged() {
    // A comment documenting a `skip:` parameter can satisfy the `pagination` pattern's `\bskip\s*:` shape even with no real pagination call in the function.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "export async function listUsersDocumented(page: number) {\n  // Note: skip: this function currently loads everything, pagination not yet implemented\n  return [];\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "pagination-no-orderby").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pagination_ok_marker_directly_above_the_pagination_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function listUsersMarked(page: number) {\n  // zzop-pagination-no-orderby-ok: single-admin dashboard, deterministic dataset snapshot\n  return prisma.user.findMany({ skip: page * 20, take: 20 });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "pagination-no-orderby").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn apollo_skip_option_with_no_find_many_in_function_is_not_flagged() {
    // Apollo Client's `useQuery(..., { skip: boolean })` flag shares the `skip:` option-name shape with
    // Prisma's row-offset pagination option but has nothing to do with a database; requiring a `.findMany(` co-occurrence excludes it.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/hooks/usePageLayout.tsx",
        "declare function useQuery(query: unknown, options: { skip: boolean }): { data: unknown };\ndeclare function isDefined(v: unknown): boolean;\ndeclare const pageLayoutQuery: unknown;\ndeclare const isOnPageLayoutPage: boolean;\ndeclare const pageLayoutId: string | undefined;\nexport function usePageLayout() {\n  return useQuery(pageLayoutQuery, { skip: !isOnPageLayoutPage || !isDefined(pageLayoutId) });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "pagination-no-orderby").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_paginated_repository_method_anchors_on_the_query_not_the_parameter_list() {
    // The finding's LINE must be the `findMany` call, not the signature that declares `skip`/`take` as
    // parameters. A TypeScript parameter type annotation (`skip: number`) satisfies the `\bskip\s*:`
    // shape, it is the FIRST line of the body span, and `method-scan` anchors on the first qualifying
    // trigger line — so a reader was sent to a signature that carries no query at all. Blind-corpus
    // subjects: cal.com `apps/api/v2/src/modules/webhooks/webhooks.repository.ts` 60/68/76 and
    // `apps/api/v2/src/platform/schedules/schedules_2024_06_11/schedules.repository.ts:233`.
    //
    // NOTE the shorthand: the query passes `skip,`/`take,` with no colon at all, so the pagination
    // pattern matches ONLY in the parameter list. Anchoring on the co-occurring `.findMany(` instead is
    // therefore the only spelling that both moves the line AND keeps the finding.
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/webhooks.repository.ts",
        "declare const prisma: any;\nexport class WebhooksRepository {\n  async getUserWebhooksPaginated(userId: number, skip: number, take: number) {\n    return prisma.webhook.findMany({\n      where: { userId },\n      skip,\n      take,\n    });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "pagination-no-orderby");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(
        h[0].line, 4,
        "must anchor on the findMany call, not the signature at line 3: {:?}",
        out.findings
    );
}

// --- unbounded-user-limit ---

#[test]
fn take_sourced_directly_from_req_query_is_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const req: any;\nexport async function listUsers() {\n  return prisma.user.findMany({ take: Number(req.query.limit) });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "unbounded-user-limit");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn take_clamped_with_math_min_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const req: any;\nexport async function listUsersClamped() {\n  return prisma.user.findMany({ take: Math.min(50, Number(req.query.limit)) });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unbounded-user-limit").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn limit_ok_marker_directly_above_the_take_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const req: any;\nexport async function listUsersMarked() {\n  // zzop-unbounded-user-limit-ok: internal admin tool, request volume trusted\n  return prisma.user.findMany({ take: Number(req.query.limit) });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unbounded-user-limit").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- skip_comment_lines + test-path file_exclude_pattern ---
// A commented-out bulk-write call must not fire `update-delete-no-where`, and every DB-write rule in this
// pack excludes test-fixture paths (a test writing against a mock DB is not a production bug).

#[test]
fn update_many_call_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function bulkArchive() {\n  // prisma.order.updateMany({ data: { archived: true } }) -- old approach, replaced below\n  return 0;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn take_sourced_from_req_query_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/__tests__/service.test.ts",
        "declare const prisma: any;\ndeclare const req: any;\nexport async function listUsers() {\n  return prisma.user.findMany({ take: Number(req.query.limit) });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unbounded-user-limit").is_empty(),
        "{:?}",
        out.findings
    );
}

/// §27 ordering pin (2026-08-26). The rule's own measured counterexample — an ordering spread in from a
/// helper, so the sort IS applied and no `orderBy` token is spelled — sat ~360 bytes behind "Add an
/// `orderBy:`". A reader who adds a second ordering on top of the first ships a query sorted by the wrong
/// column. The remedy now carries the premise; the counterexample still follows it, unchanged.
/// Detection is untouched — the count and line below are the pre-edit ones.
#[test]
fn pagination_no_orderby_message_puts_the_stable_sort_condition_before_the_orderby_imperative() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function listUsers(page: number) {\n  return prisma.user.findMany({ skip: page * 20, take: 20 });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "pagination-no-orderby");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    assert_disqualifier_summary_precedes_imperative(
        "pagination-no-orderby",
        &h[0].message,
        "IF NO STABLE SORT IS ALREADY APPLIED",
        "add an `orderBy:`",
        "the sort is right there in the spread, one call away",
    );
}

/// The SIBLING-CONTRADICTION pin (2026-08-27). The §27 pin above fixed the ORDER of this rule's own
/// disqualifier; this one fixes what the remedy tells the reader to write.
///
/// Measured on cal.com: `pagination-no-orderby` fires 13 times and `orderby-unindexed` 3, and the two
/// sets do not intersect — not at `file:line`, not even at file level. That is latency, not safety: the
/// prescription said "a unique or otherwise stable column", and the "otherwise stable" branch is
/// exactly what the sibling rule reports on. `orderby-unindexed` fires when the sort column is neither
/// `@id`/`@unique` nor the LEADING column of an `@@index`/`@@unique` — which `createdAt`, the usual
/// answer to "otherwise stable", generally is not. Following this remedy is what creates the
/// intersection, so each of the 13 firings is a latent sibling finding.
///
/// All three cal.com firings of the sibling are on `createdAt`, which is the measurement, not an
/// illustration: `packages/platform/examples/base/src/pages/api/{get-users,managed-user,oauth2-user}.ts`.
///
/// The REVERSE direction was measured too, on a synthetic Prisma tree, because a repair that hands the
/// contradiction back the other way is not a repair: applying `orderby-unindexed`'s own remedy
/// (`@@index([createdAt])`) cleared it 1 -> 0 and produced ZERO new findings of any rule — it is a
/// `.prisma` edit and this rule reads `.ts` function bodies, so it cannot reach one.
///
/// The tiebreaker clause names its own limit for a reason found in that same measurement: switching to
/// the array form `orderBy: [{ createdAt }, { id }]` ALSO takes `orderby-unindexed` to 0 with no index
/// added at all, because `single_field_order_by` decides only the single-key object form
/// (`rules/native/rules-schema/src/join.rs`). A remedy that quietly silences the sibling by leaving its
/// decidable subset would be the same defect wearing the other face, so the message says which key the
/// index requirement is about and that the silence is a boundary rather than a second opinion.
///
/// Detection is untouched — the count and line asserted here are the pre-edit ones.
#[test]
fn pagination_no_orderby_remedy_names_the_index_its_sibling_rule_judges() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function listUsers(page: number) {\n  return prisma.user.findMany({ skip: page * 20, take: 20 });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "pagination-no-orderby");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    let msg = &h[0].message;
    assert!(
        !msg.contains("unique or otherwise stable"),
        "pagination-no-orderby: the remedy is back to `unique or otherwise stable`, whose second \
         branch is precisely what `orderby-unindexed` reports as a full table scan: {msg}"
    );
    for needle in [
        "LEADING column of an `@@index`",
        "orderby-unindexed",
        "add `@@index([column])` in the same change",
        "tiebreaker",
        "the FIRST key of that list",
        "decidable-subset boundary rather than a second opinion",
    ] {
        assert!(
            msg.contains(needle),
            "pagination-no-orderby: the remedy lost {needle:?}, so following it can still produce the \
             sibling finding it was rewritten to avoid: {msg}"
        );
    }
    // ORDER: the index requirement is part of the INSTRUCTION, so it has to sit between the verb and
    // the rule's own limitation disclosure — not appended after it, where a reader who has already
    // acted never arrives. The invalidation probe is exactly that move: put the clause at the tail of
    // the message, every token still present and spelled once, and this assertion goes red alone.
    let imperative = msg.find("add an `orderBy:`").expect("imperative missing");
    let index_clause = msg
        .find("LEADING column of an `@@index`")
        .expect("index clause missing");
    let disclosure = msg
        .find("The predicate is honest")
        .expect("limitation disclosure missing");
    assert!(
        imperative < index_clause && index_clause < disclosure,
        "pagination-no-orderby: the remedy is at {imperative}, its index requirement at \
         {index_clause} and this rule's limitation disclosure at {disclosure} — the requirement is \
         part of the instruction and has to be read with it, not appended behind the caveats: {msg}"
    );
}
