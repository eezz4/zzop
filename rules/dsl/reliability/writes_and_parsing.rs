use crate::{hits, scan, TempDir};

// --- promise-all-and-writes ---

#[test]
fn promise_all_wrapping_create_calls_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/save.ts",
        "declare const db: any;\nexport async function saveAll(items: any[]) {\n  return Promise.all(items.map((i) => db.record.create({ data: i })));\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "promise-all-and-writes");
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
        hits(&out, "promise-all-and-writes").is_empty(),
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
        hits(&out, "promise-all-and-writes").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn promise_all_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/save.ts",
        "declare const db: any;\nexport async function saveAll(items: any[]) {\n  // zzop-promise-all-and-writes-ok: single-tenant seed script, re-run is idempotent\n  return Promise.all(items.map((i) => db.record.create({ data: i })));\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "promise-all-and-writes").is_empty(),
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
        hits(&out, "promise-all-and-writes").is_empty(),
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
        hits(&out, "promise-all-and-writes").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- the result-shape counter-indication (2026-08-25) ---
//
// The remedy used to read "Wrap the writes in a transaction, or use `Promise.allSettled(...)` with
// compensation logic", and its caveat warned about the WRONG AXIS: it said the rule cannot prove the
// writes sit inside the array, which is a precision disclosure, not a warning about the fix. Swapping in
// `Promise.allSettled(...)` changes the RESOLVED VALUE — settlement wrappers instead of the values — and
// on cal.com apps/web/app/api/cron/bookingReminder/route.ts:117 the awaited array becomes a calendar
// event's `attendees` list, so the swap ships a corrupted event with nothing failing loudly. Pinned the
// same way `write-in-loop-no-tx`'s condition is pinned one pack over.

#[test]
fn promise_all_message_warns_about_the_result_shape_before_naming_all_settled() {
    let dir = TempDir::new("zzop-be-rel");
    // The bookingReminder shape, reduced: the awaited array is CONSUMED, not discarded.
    dir.write(
        "src/reminder.ts",
        "declare const db: any;\ndeclare const attendees: any[];\ndeclare function translate(a: any): Promise<any>;\nexport async function remind() {\n  const attendeesList = await Promise.all(attendees.map((a) => translate(a)));\n  await db.reminderMail.create({ data: { attendees: attendeesList } });\n  return attendeesList;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "promise-all-and-writes");
    // Still fires — sentence repair only.
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);

    let m = &h[0].message;
    for needle in [
        "IF ANY CODE CONSUMES THE RESOLVED ARRAY",
        "DO NOT SWAP IN `Promise.allSettled(...)`",
        "settlement wrappers",
        // Which consumers the swap is actually silent on. Until 2026-08-25 this pinned "no compile
        // error", carried by an exhibit that was the one site where the hazard CANNOT happen:
        // cal.com route.ts:119 annotates `const evt: CalendarEvent` and :138 assigns the awaited
        // array to `attendees`, whose type is `Person[]` (packages/types/Calendar.d.ts:32,173) — so
        // `tsc` catches that swap. Right warning, wrong exhibit. The claim is now scoped to the
        // consumers that really do stay quiet, with the compiler named as the GOOD case.
        "the swap is silent exactly where the value is `any`",
    ] {
        assert!(
            m.contains(needle),
            "promise-all-and-writes' remedy no longer warns that `Promise.allSettled(...)` changes the \
             RESOLVED VALUE — missing {needle:?}. The caveat it used to carry is about write location, \
             which is a different axis and does not stop this. In: {m}"
        );
    }

    // Condition before imperative, same discipline as the sibling rule.
    let condition = m
        .find("IF THE RESOLVED ARRAY IS DISCARDED")
        .expect("the safe branch must be stated as a condition, not as a bare imperative");
    let settled = m
        .find("`Promise.allSettled(...)`")
        .expect("the remedy must still name the settled fix for the discard case");
    assert!(
        condition < settled,
        "`Promise.allSettled(...)` is named before the condition that decides whether it is safe \
         (condition at {condition}, mention at {settled}). In: {m}"
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
        "export function handleBody(req: any) {\n  // zzop-json-parse-no-try-ok: upstream gateway already validates JSON shape\n  const parsed = JSON.parse(req.body);\n  return parsed;\n}\n",
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

// --- await-inside-promise-all-array ---

#[test]
fn await_inside_a_promise_all_array_literal_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/load.ts",
        "declare function getA(): Promise<number>;\ndeclare function getB(): Promise<number>;\nexport async function loadBoth() {\n  return await Promise.all([await getA(), await getB()]);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "await-inside-promise-all-array");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn await_producing_the_promise_all_array_itself_is_not_flagged() {
    // FP-adversarial pin: `await` resolves `buildTasks()` to the ARRAY passed to `Promise.all`, which is
    // the correct, fully-parallel shape — there is no `await` between the array literal's `[` and `]`
    // (there is no `[` at all here), so the line-scan's `\[[^\]]*\bawait\b` never matches.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/load.ts",
        "declare function buildTasks(): Promise<Promise<number>[]>;\nexport async function loadAll() {\n  return await Promise.all(await buildTasks());\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "await-inside-promise-all-array").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn promise_all_await_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/load.ts",
        "declare function getA(): Promise<number>;\ndeclare function getB(): Promise<number>;\nexport async function loadBoth() {\n  // zzop-await-inside-promise-all-array-ok: two independent one-off calls, serial cost is negligible here\n  return await Promise.all([await getA(), await getB()]);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "await-inside-promise-all-array").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- fs-check-then-use ---
//
// Co-occurrence check only: a `check` label (existsSync/access/stat) and a `use` label (writeFile/open/rename/...)
// in the same function body. It does not confirm the two calls touch the same path, or that the check runs
// before the write — stated plainly in the rule's message.

#[test]
fn exists_check_followed_by_write_with_no_exclusive_flag_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/save.ts",
        "import fs from \"fs\";\n\nexport function saveIfAbsent(p: string, data: string) {\n  if (!fs.existsSync(p)) {\n    fs.writeFileSync(p, data);\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "fs-check-then-use");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

#[test]
fn exists_check_followed_by_exclusive_create_write_is_not_flagged() {
    // Adversarial negative: the exclusive-create flag (`wx`) is exactly the fix the rule's message
    // recommends — it fails atomically if the path already exists, closing the TOCTOU window, so the
    // `absent: exclusive` veto must suppress this even though check+use still co-occur.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/save.ts",
        "import fs from \"fs\";\n\nexport function saveIfAbsent(p: string, data: string) {\n  if (!fs.existsSync(p)) {\n    fs.writeFileSync(p, data, { flag: 'wx' });\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fs-check-then-use").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn exists_check_guarding_only_a_read_is_not_flagged() {
    // Adversarial negative: `existsSync` guards a `readFileSync` — a read, not a member of the `use`
    // pattern's create/write call set — so the `use` label never appears and the rule has nothing to
    // trigger on at all.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/load.ts",
        "import fs from \"fs\";\n\nexport function loadIfPresent(p: string) {\n  if (fs.existsSync(p)) {\n    return fs.readFileSync(p, 'utf8');\n  }\n  return null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fs-check-then-use").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn fs_check_use_ok_marker_above_the_write_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/save.ts",
        "import fs from \"fs\";\n\nexport function saveIfAbsent(p: string, data: string) {\n  if (!fs.existsSync(p)) {\n    // zzop-fs-check-then-use-ok: single-writer batch job, no concurrent access to this path\n    fs.writeFileSync(p, data);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fs-check-then-use").is_empty(),
        "{:?}",
        out.findings
    );
}

// ORDER-GATE pin: seals the `after: check` gate — a write that PRECEDES the only existence check is a
// write-then-verify, the reverse of the CWE-367 check-then-use window the id names. Before the gate,
// plain co-occurrence in the same function fired.
#[test]
fn an_fs_write_that_precedes_the_only_check_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/writeThenVerify.ts",
        "import fs from \"fs\";\n\nexport function writeThenVerify(p: string, data: string) {\n  fs.writeFileSync(p, data);\n  return fs.existsSync(p);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fs-check-then-use").is_empty(),
        "{:?}",
        out.findings
    );
}
