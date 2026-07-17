use super::*;

fn keys(rel: &str, src: &str) -> Vec<String> {
    extract_db_table_consumes(rel, src)
        .into_iter()
        .map(|c| c.key.unwrap())
        .collect()
}

#[test]
fn matches_read_and_write_methods() {
    let src = "async function f() {\n  await getPrisma().order.findMany({});\n  await getPrisma().user.create({});\n}\n";
    assert_eq!(keys("a.ts", src), vec!["table:order", "table:user"]);
}

#[test]
fn ignores_non_getter_receiver() {
    // A bare `foo.bar.baz()` (no getPrisma() anchor) must not match.
    let src = "function f() {\n  foo.bar.findMany();\n  cache.get('x').then(() => 1);\n}\n";
    assert!(keys("a.ts", src).is_empty());
}

#[test]
fn ignores_getter_with_args() {
    let src = "function f() {\n  getPrisma(tenant).order.findMany();\n}\n";
    assert!(keys("a.ts", src).is_empty());
}

#[test]
fn then_chain_does_not_double_count() {
    let src = "function f() {\n  getPrisma().order.findMany().then(r => r);\n}\n";
    assert_eq!(keys("a.ts", src), vec!["table:order"]);
}

// --- extract_query_call_sites ---

fn sites(rel: &str, src: &str) -> Vec<QueryCallSite> {
    extract_query_call_sites(rel, src)
}

#[test]
fn finds_find_many_with_model_and_line() {
    let src = "export function list() {\n  return getPrisma().item.findMany({ where: { ownerId: 1 } });\n}\n";
    let out = sites("src/domains/item/repo.ts", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].model, "Item");
    assert_eq!(out[0].method, "findMany");
    assert_eq!(out[0].file, "src/domains/item/repo.ts");
    assert_eq!(out[0].line, 2);
    assert_eq!(out[0].call_text, "({ where: { ownerId: 1 } })");
}

#[test]
fn captures_balanced_span_across_nested_braces() {
    let src = "export function list() {\n  return getPrisma().item.findMany({\n    where: { ownerId: 1, meta: { a: fn(1, 2) } },\n    orderBy: { name: 'asc' },\n  });\n}\n";
    let out = sites("a.ts", src);
    assert_eq!(out.len(), 1);
    assert!(out[0].call_text.contains("orderBy"));
    assert!(out[0].call_text.starts_with('('));
    assert!(out[0].call_text.ends_with(')'));
}

#[test]
fn multiple_sites_same_file_correct_lines() {
    let src = "export function a() {\n  return getPrisma().item.findMany({});\n}\n\nexport function b() {\n  return getPrisma().item.count({});\n}\n";
    let out = sites("a.ts", src);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].line, 2);
    assert_eq!(out[0].method, "findMany");
    assert_eq!(out[1].line, 6);
    assert_eq!(out[1].method, "count");
}

#[test]
fn ignores_non_query_methods() {
    // `create` is a db-table consume but not a query call site — the 4-method filter rejects it.
    let src = "export function f() { return getPrisma().item.create({ data: {} }); }\n";
    assert!(sites("a.ts", src).is_empty());
}

#[test]
fn model_capitalization() {
    let src = "function f() { return getPrisma().userProfile.findFirst({}); }\n";
    let out = sites("a.ts", src);
    assert_eq!(out[0].model, "UserProfile");
}

#[test]
fn then_chain_on_query_method_does_not_double_count() {
    let src = "function f() {\n  getPrisma().order.findMany().then(r => r);\n}\n";
    let out = sites("a.ts", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].method, "findMany");
}

#[test]
fn test_files_are_skipped_by_both_extractors() {
    // A source that would otherwise match both extractors must yield nothing when the path is a
    // test/spec file — its DB access is not deployed coupling or real query surface.
    let src = "async function f() {\n  await getPrisma().order.findMany({});\n}\n";
    assert!(extract_db_table_consumes("src/order.test.ts", src).is_empty());
    assert!(extract_query_call_sites("src/order.test.ts", src).is_empty());
}

// --- bare-receiver consumes (BARE-SINGLETON CONSUMES follow-up) ---

#[test]
fn be_express_local_prisma_client_import_matches() {
    // The exact be-express idiom: a route/service file imports the default export of a LOCAL module
    // whose specifier contains "prisma" and calls `prisma.<model>.<method>(...)` directly (no
    // `getPrisma()` anchor at all).
    let src = "import prisma from '../../../prisma/prisma-client';\n\nexport const getArticles = async () => {\n  const c = await prisma.article.count({});\n  const a = await prisma.article.findMany({});\n  return { c, a };\n};\n";
    assert_eq!(
        keys("src/app/routes/article/article.service.ts", src),
        vec!["table:article", "table:article"]
    );
}

#[test]
fn in_file_new_prisma_client_matches() {
    // `import { PrismaClient } from '@prisma/client'; const prisma = new PrismaClient();` — the
    // single-file setup idiom, receiver bound via `new PrismaClient(...)`.
    let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() {\n  await prisma.user.create({});\n}\n";
    assert_eq!(keys("a.ts", src), vec!["table:user"]);
}

#[test]
fn global_fallback_new_prisma_client_matches() {
    // The `global.prisma || new PrismaClient()` fallback idiom (be-express's own
    // `src/prisma/prisma-client.ts` shape) still counts as `new PrismaClient(...)` evidence.
    let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = global.prisma || new PrismaClient();\nasync function f() {\n  await prisma.order.findMany({});\n}\n";
    assert_eq!(keys("a.ts", src), vec!["table:order"]);
}

#[test]
fn no_prisma_evidence_bare_receiver_does_not_match() {
    // A bare `prisma.article.findMany()` with NO import evidence anywhere in the file must never match
    // — the precision guard this whole follow-up hinges on.
    let src =
        "async function f() {\n  const a = await prisma.article.findMany({});\n  return a;\n}\n";
    assert!(keys("a.ts", src).is_empty());
}

#[test]
fn unrelated_local_import_does_not_grant_evidence() {
    // A relative import whose specifier does NOT mention "prisma" grants no evidence, even though the
    // call shape otherwise matches.
    let src = "import prisma from '../db/client';\nasync function f() {\n  await prisma.article.findMany({});\n}\n";
    assert!(keys("a.ts", src).is_empty());
}

#[test]
fn type_only_prisma_client_import_grants_no_evidence() {
    // `import type { PrismaClient }` has no runtime value — using the type name as a bare receiver
    // (impossible in real code, but exercised here as a boundary check) must not match.
    let src = "import type { PrismaClient } from '@prisma/client';\nasync function f() {\n  await PrismaClient.article.findMany({});\n}\n";
    assert!(keys("a.ts", src).is_empty());
}

#[test]
fn getprisma_and_bare_receiver_both_recognized_in_same_file() {
    // Both anchor forms coexist without interference.
    let src = "import prisma from '../prisma/prisma-client';\nasync function f() {\n  await getPrisma().order.findMany({});\n  await prisma.user.create({});\n}\n";
    assert_eq!(keys("a.ts", src), vec!["table:order", "table:user"]);
}

#[test]
fn bare_receiver_widens_query_call_sites_too() {
    let src = "import prisma from '../prisma/prisma-client';\nexport function list() {\n  return prisma.item.findMany({ where: { ownerId: 1 } });\n}\n";
    let out = sites("src/domains/item/repo.ts", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].model, "Item");
    assert_eq!(out[0].method, "findMany");
}
