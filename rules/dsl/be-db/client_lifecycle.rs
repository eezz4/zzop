//! `client-per-request` + `connection-no-release` tests (split from `be-db.rs`).

use super::*;

// --- client-per-request ---

#[test]
fn new_prisma_client_inside_request_handler_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/handler.ts",
        "declare class PrismaClient { user: any; }\nexport async function handleRequest(req: any, res: any) {\n  const prisma = new PrismaClient();\n  const users = await prisma.user.findMany();\n  res.status(200).json(users);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "client-per-request");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn module_top_level_singleton_prisma_client_is_not_flagged() {
    // A module-top-level `new PrismaClient()` has no enclosing function span, so it's never scanned here regardless of the handler-context co-pattern (see module doc).
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/handler.ts",
        "declare class PrismaClient { user: any; }\nconst prisma = new PrismaClient();\nexport async function handleRequest(req: any, res: any) {\n  const users = await prisma.user.findMany();\n  res.json(users);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-per-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn prisma_client_ok_marker_directly_above_the_new_client_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/handler.ts",
        "declare class PrismaClient { user: any; }\nexport async function handleAdminRequest(req: any, res: any) {\n  // prisma-client-ok: cold-start admin tool, single-invocation script\n  const prisma = new PrismaClient();\n  const users = await prisma.user.findMany();\n  res.status(200).json(users);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-per-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn fetch_response_variable_named_res_is_not_handler_context_evidence() {
    // A bare `res` identifier bound to a fetch Response satisfied the old
    // naive handler-context vocabulary (`\b(req|res|ctx|request|reply)\b`), making a `new PrismaClient()`
    // call inside a plain data-fetching helper look like a request handler. The new evidence requires a
    // `req.`/`request.` member access or a response-API CALL fetch's own Response shape doesn't have
    // (`res.json(`/`res.text(` are deliberately excluded — see the rule's message), so this stays silent.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/jobs/refresh.ts",
        "declare class PrismaClient { user: any; }\nexport async function refresh() {\n  const prisma = new PrismaClient();\n  const res = await fetch(\"https://example.com/data\");\n  const data = await res.json();\n  await prisma.user.create({ data });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-per-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_with_only_bare_res_json_evidence_is_now_silent_accepted_false_negative() {
    // Documented limitation (see be-db.json's client-per-request message): `res.json(x)` alone no longer
    // counts as handler-context evidence — deliberately excluded because a fetch `Response` also has
    // `.json()`, the same false-positive class the vocabulary swap above fixes. A handler whose ONLY
    // evidence is `res.json(...)` (no `req.`/`request.` member access, no other response-API call) is now an
    // accepted false negative — pinned here so the tradeoff can't silently drift.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/handler.ts",
        "declare class PrismaClient { user: any; }\nexport async function handleRequest(req: any, res: any) {\n  const prisma = new PrismaClient();\n  const users = await prisma.user.findMany();\n  res.json(users);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-per-request").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- connection-no-release ---

#[test]
fn pool_connect_with_no_release_anywhere_in_function_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function runQuery(sql: string) {\n  const conn = await pool.connect();\n  await conn.query(sql);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "connection-no-release");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn get_connection_with_no_release_anywhere_in_function_is_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function runQueryGetConnection(sql: string) {\n  const conn = await pool.getConnection();\n  await conn.query(sql);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "connection-no-release");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn pool_connect_released_in_finally_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function runQueryReleased(sql: string) {\n  const conn = await pool.connect();\n  try {\n    await conn.query(sql);\n  } finally {\n    conn.release();\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "connection-no-release").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pool_connect_returned_to_caller_is_not_flagged() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function acquireConnectionForCaller() {\n  const conn = await pool.connect();\n  return conn;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "connection-no-release").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pool_connect_bound_via_await_using_declaration_is_not_flagged() {
    // Review calibration pin: TS explicit resource management (`using` / `await using`,
    // Symbol.dispose) releases on scope exit — the message promises this shape is recognized.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function withManagedConnection() {\n  await using conn = await pool.connect();\n  await conn.query('SELECT 1');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "connection-no-release").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pool_connect_released_only_inside_a_helper_function_is_still_flagged() {
    // Documents the message's own honest limitation: `MethodScan`'s `absent` veto only sees text inside
    // THIS function's own span. `releaseConn(conn)` is a bare identifier call, not `.release(`/`.destroy(`/
    // `.end(`, so it never satisfies the `released` veto even though the connection genuinely is released,
    // one call away, inside `releaseConn`. This is a real false positive the message warns readers about
    // ("verify before refactoring") rather than a bug — pinned here so the claim can't silently drift.
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\ndeclare function releaseConn(conn: any): void;\nexport async function runQueryHelperRelease(sql: string) {\n  const conn = await pool.connect();\n  await conn.query(sql);\n  releaseConn(conn);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "connection-no-release");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn connection_release_ok_marker_directly_above_the_acquire_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-db");
    dir.write(
        "src/service.ts",
        "declare const pool: any;\nexport async function runQueryMarked(sql: string) {\n  // connection-release-ok: pooled test harness connection, released by the test teardown hook\n  const conn = await pool.connect();\n  await conn.query(sql);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "connection-no-release").is_empty(),
        "{:?}",
        out.findings
    );
}
