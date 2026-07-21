//! `pagination-no-orderby` + `unbounded-user-limit` + comment-skip/test-path exclusion tests (split from `be-db.rs`).

use super::*;

// --- pagination-no-orderby ---

#[test]
fn skip_take_pagination_with_no_orderby_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
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
    let dir = TempDir::new("zzop-be-db");
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
    let dir = TempDir::new("zzop-be-db");
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
    let dir = TempDir::new("zzop-be-db");
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
    let dir = TempDir::new("zzop-be-db");
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
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\nexport async function listUsersMarked(page: number) {\n  // pagination-ok: single-admin dashboard, deterministic dataset snapshot\n  return prisma.user.findMany({ skip: page * 20, take: 20 });\n}\n",
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
    let dir = TempDir::new("zzop-be-db");
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

// --- unbounded-user-limit ---

#[test]
fn take_sourced_directly_from_req_query_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
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
    let dir = TempDir::new("zzop-be-db");
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
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const req: any;\nexport async function listUsersMarked() {\n  // limit-ok: internal admin tool, request volume trusted\n  return prisma.user.findMany({ take: Number(req.query.limit) });\n}\n",
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
    let dir = TempDir::new("zzop-be-db");
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
    let dir = TempDir::new("zzop-be-db");
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
