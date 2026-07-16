use crate::{hits, scan, TempDir};

// --- promise-all-writes ---

#[test]
fn promise_all_wrapping_create_calls_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/save.ts",
        "declare const db: any;\nexport async function saveAll(items: any[]) {\n  return Promise.all(items.map((i) => db.record.create({ data: i })));\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "promise-all-writes");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn promise_all_wrapping_only_reads_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/load.ts",
        "declare const db: any;\nexport async function loadAll(ids: string[]) {\n  return Promise.all(ids.map((id) => db.record.findUnique({ where: { id } })));\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "promise-all-writes").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn promise_all_settled_wrapping_writes_is_not_flagged() {
    // Precision-limit case from the rule's message: `Promise\.all\s*\(` does not match `Promise.allSettled(` (the text right after `all` is `Settled`, not whitespace/`(`).
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/save.ts",
        "declare const db: any;\nexport async function saveAll(items: any[]) {\n  return Promise.allSettled(items.map((i) => db.record.create({ data: i })));\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "promise-all-writes").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn promise_all_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/save.ts",
        "declare const db: any;\nexport async function saveAll(items: any[]) {\n  // promise-all-ok: single-tenant seed script, re-run is idempotent\n  return Promise.all(items.map((i) => db.record.create({ data: i })));\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "promise-all-writes").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn file_system_access_api_create_writable_alongside_promise_all_is_not_flagged() {
    // `fileHandle.createWritable()` (browser File System Access API, zero DB calls) would match an unscoped `\.(create|update|delete|upsert)\w*\s*\(` pattern via its `\w*` suffix wildcard — "create" + "Writable" + "(" satisfies it even though this has nothing to do with a database.
    // The receiver-scoped pattern (`\b(?:prisma|db|tx|...)\.` + an exact `create`/`update`/`delete`/`upsert` suffix, optionally `Many`) does not match a bare `createWritable(` call on an unscoped receiver.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/modules/fileDialog.ts",
        "declare const fileHandles: any[];\nexport async function saveAll() {\n  const files = await Promise.all(fileHandles.map((v) => v.getFile()));\n  const fileHandle = fileHandles[0];\n  const writable = await fileHandle.createWritable();\n  await writable.write(files[0]);\n  return files;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "promise-all-writes").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn promise_all_with_a_store_suffixed_receiver_is_not_flagged() {
    // Documented precision limit: the receiver allow-list (`prisma`/`db`/`tx`/`client`/`repo(sitory|sitories)?s?`) does not cover a `*Store`-suffixed naming convention. This also happens to suppress the "Promise.all with only ONE write + one read" FP shape when the write's receiver is `*Store`-named — see the rule's message for the tradeoff.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/api/comments.ts",
        "declare const supportTicketCommentStore: any;\ndeclare const userLookup: any;\nexport async function addComment(ticketId: string, userId: string) {\n  return Promise.all([\n    supportTicketCommentStore.create({ ticketId }),\n    userLookup.findById(userId),\n  ]);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "promise-all-writes").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- json-parse-no-try ---

#[test]
fn json_parse_of_request_body_without_try_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/handler.ts",
        "export function handleBody(req: any) {\n  const parsed = JSON.parse(req.body);\n  return parsed;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "json-parse-no-try");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn json_parse_of_request_body_inside_try_catch_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/handler.ts",
        "export function handleBody(req: any) {\n  try {\n    const parsed = JSON.parse(req.body);\n    return parsed;\n  } catch (err) {\n    return null;\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "json-parse-no-try").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn json_parse_ok_marker_above_the_parse_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/handler.ts",
        "export function handleBody(req: any) {\n  // json-parse-ok: upstream gateway already validates JSON shape\n  const parsed = JSON.parse(req.body);\n  return parsed;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "json-parse-no-try").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn json_parse_of_bare_identifier_body_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/handler.ts",
        "export function handle({ body }: any) {\n  const parsed = JSON.parse(body);\n  return parsed;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "json-parse-no-try");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn json_parse_of_own_previously_stringified_cache_is_not_flagged() {
    // A self-written cache read back via `JSON.parse(cached)`/`JSON.parse(cached.body)` must not fire: an unscoped co-occurrence check would trigger on an unrelated `payload`/`body` identifier existing *anywhere* in the same function (here, `const payload = await fetcher();`, and `cached.body`'s property access), not because the JSON.parse call itself touched external input.
    // The check is anchored to the parse call's own argument, so neither `cached` nor `cached.body` (a property access on an unrelated receiver, not the bare `body` identifier) matches.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/cache/getCachedList.ts",
        "declare const redis: any;\ndeclare function fetcher(): Promise<unknown>;\nexport async function getCachedList() {\n  const cached = await redis.get(\"list\");\n  if (cached) {\n    return JSON.parse(cached.body ?? cached);\n  }\n  const payload = await fetcher();\n  const body = JSON.stringify(payload);\n  await redis.set(\"list\", body);\n  return payload;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "json-parse-no-try").is_empty(),
        "{:?}",
        out.findings
    );
}
