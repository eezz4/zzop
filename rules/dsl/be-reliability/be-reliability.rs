//! End-to-end tests for `rules/dsl/be-reliability/be-reliability.json` — exercised via `zpz_engine::analyze_tree` so `Matcher::MethodScan` rules run against real parser-derived `SourceSymbol` body spans (not hand-built spans), same convention as `sql/sql.rs`/`http/http.rs`.
//!
//! Covers all rules in the pack: `async-route-no-catch`, `sync-fs-in-handler`, `await-in-map`, `promise-all-writes`, `json-parse-no-try`, `fetch-no-timeout`, `process-exit-in-lib` (method-scan); `env-nonnull-assert`, `debug-true-committed`, `body-limit-missing`, `console-in-be`, `interval-no-clear` (line-scan, uses the `require_file_absent` DSL extension), `env-outside-config` (line-scan).
//!
//! `fetch-no-timeout` scopes to backend files via a content-based `require_file` pre-gate (server-framework import / server-runtime API / Workers module shape / D1 prepared-statement call) rather than a path heuristic, so a standalone backend repo with no `be`/`api`/`server`-ish path segment is still in scope.
//!
//! Each rule has >=1 positive fixture (count + line asserted), >=1 realistic negative, and at least one `suppress_marker` case is covered.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zpz_core::{load_dsl_packs, RulePackDef};
use zpz_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — copied verbatim from `sql/sql.rs`).
struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Loads the real `rules/dsl/be-reliability/be-reliability.json` from the repo, filtered to just the `be-reliability` pack so this test is unaffected by sibling packs under concurrent development (same convention as `http/http.rs`).
///
/// `CARGO_MANIFEST_DIR` is the `rules` crate root (`rules/Cargo.toml`), so `dsl/` is `rules/dsl` — this pack's own `be-reliability.json` lives one level down, at `rules/dsl/be-reliability/be-reliability.json`.
fn be_reliability_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "be-reliability")
        .expect("be-reliability pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "be-reliability-fixture".to_string(),
        packs: vec![be_reliability_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zpz_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("be-reliability/{rule}"))
        .collect()
}

// --- async-route-no-catch ---

#[test]
fn async_route_without_try_catch_or_next_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/routes.ts",
        "declare function getItems(): Promise<unknown>;\nexport function registerRoutes(app: any) {\n  app.get(\"/items\", async (req: any, res: any) => {\n    const items = await getItems();\n    res.json(items);\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "async-route-no-catch");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn async_route_with_try_catch_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/routes.ts",
        "declare function getItems(): Promise<unknown>;\ndeclare function next(e: unknown): void;\nexport function registerRoutes(app: any) {\n  app.get(\"/items\", async (req: any, res: any) => {\n    try {\n      const items = await getItems();\n      res.json(items);\n    } catch (err) {\n      next(err);\n    }\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "async-route-no-catch").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn async_route_registered_at_module_top_level_is_not_scanned() {
    // Precision-limit case from the rule's message: no enclosing function body -> no symbol span -> method-scan silently skips it.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/routes.ts",
        "declare const app: any;\ndeclare function getItems(): Promise<unknown>;\napp.get(\"/items\", async (req: any, res: any) => {\n  const items = await getItems();\n  res.json(items);\n});\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "async-route-no-catch").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn route_catch_ok_marker_above_the_route_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/routes.ts",
        "declare function getItems(): Promise<unknown>;\nexport function registerRoutes(app: any) {\n  // route-catch-ok: reviewed, failures are non-critical telemetry reads\n  app.get(\"/items\", async (req: any, res: any) => {\n    const items = await getItems();\n    res.json(items);\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "async-route-no-catch").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- sync-fs-in-handler ---

#[test]
fn sync_read_file_in_request_handler_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/handler.ts",
        "import { readFileSync } from \"fs\";\nexport function handler(req: any, res: any) {\n  const data = readFileSync(\"./config.json\", \"utf8\");\n  res.json(data);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sync-fs-in-handler");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn sync_read_file_in_module_init_with_no_handler_context_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/config.ts",
        "import { readFileSync } from \"fs\";\nexport function loadConfig() {\n  const data = readFileSync(\"./config.json\", \"utf8\");\n  return JSON.parse(data);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sync-fs-in-handler").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sync_io_ok_marker_above_the_sync_call_suppresses_the_finding() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/handler.ts",
        "import { readFileSync } from \"fs\";\nexport function handler(req: any, res: any) {\n  // sync-io-ok: startup-time cache warm, not on the per-request path\n  const data = readFileSync(\"./config.json\", \"utf8\");\n  res.json(data);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sync-fs-in-handler").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- await-in-map ---

#[test]
fn map_async_without_promise_all_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/process.ts",
        "declare function fetchItem(id: string): Promise<unknown>;\nexport async function process(ids: string[]) {\n  return ids.map(async (id) => {\n    return await fetchItem(id);\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "await-in-map");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn map_async_wrapped_in_promise_all_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/process.ts",
        "declare function fetchItem(id: string): Promise<unknown>;\nexport async function process(ids: string[]) {\n  return Promise.all(ids.map(async (id) => {\n    return await fetchItem(id);\n  }));\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "await-in-map").is_empty(), "{:?}", out.findings);
}

#[test]
fn map_async_ok_marker_above_the_map_call_suppresses_the_finding() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/process.ts",
        "declare function notify(id: string): Promise<unknown>;\nexport async function process(ids: string[]) {\n  // map-async-ok: fire-and-forget notifications, failures logged elsewhere\n  return ids.map(async (id) => {\n    return await notify(id);\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "await-in-map").is_empty(), "{:?}", out.findings);
}

// --- env-nonnull-assert ---

#[test]
fn process_env_non_null_assertion_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/config.ts",
        "export const key = process.env.API_KEY!;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "env-nonnull-assert");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn process_env_strict_inequality_comparison_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/config.ts",
        "export function checkEnv(): boolean {\n  if (process.env.API_KEY !== undefined) {\n    return true;\n  }\n  return false;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-nonnull-assert").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn env_assert_ok_marker_above_the_assertion_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/config.ts",
        "// env-assert-ok: validated at startup in bootstrap.ts\nexport const key = process.env.API_KEY!;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-nonnull-assert").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- debug-true-committed ---

#[test]
fn debug_true_flag_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/config.ts",
        "export const config = { debug: true, name: \"app\" };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "debug-true-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn tls_reject_unauthorized_disabled_via_env_assignment_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/bootstrap.ts",
        "process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "debug-true-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn reject_unauthorized_false_in_https_agent_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/client.ts",
        "export const agent = new (require(\"https\").Agent)({ rejectUnauthorized: false });\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "debug-true-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn debug_flag_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/config.ts",
        "// debug: true was removed here, see PR #123\nexport const config = { debug: false };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "debug-true-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn debug_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/config.ts",
        "export const config = { debug: true }; // debug-ok: local dev override, gated by NODE_ENV below\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "debug-true-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tls_reject_unauthorized_disabled_in_a_playwright_e2e_helper_is_not_flagged() {
    // `NODE_TLS_REJECT_UNAUTHORIZED=0` for a local self-signed cert in a Playwright e2e helper is the intended target, not a leaked dev backdoor — same exclusion as this pack's sibling `fullstack/localhost-egress-committed`.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "e2e/globalSetup.ts",
        "process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "debug-true-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tls_reject_unauthorized_disabled_in_a_test_file_by_suffix_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/setup.test.ts",
        "process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "debug-true-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- promise-all-writes ---

#[test]
fn promise_all_wrapping_create_calls_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
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
    let dir = TempDir::new("zpz-be-rel");
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
    let dir = TempDir::new("zpz-be-rel");
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
    let dir = TempDir::new("zpz-be-rel");
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
    let dir = TempDir::new("zpz-be-rel");
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
    let dir = TempDir::new("zpz-be-rel");
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
    let dir = TempDir::new("zpz-be-rel");
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
    let dir = TempDir::new("zpz-be-rel");
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
    let dir = TempDir::new("zpz-be-rel");
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
    let dir = TempDir::new("zpz-be-rel");
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
    let dir = TempDir::new("zpz-be-rel");
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

// --- fetch-no-timeout ---
//
// Scoping by backend-looking PATH alone would fire on every bare frontend fetch (and on a chrome.runtime.getURL local-resource call) while still missing a standalone backend repo rooted at `src/` with no `be`/`api`/`server`-ish path segment.
// Instead this rule gates on file CONTENT via `require_file` — the file text must show a server-side signal (framework import, server-runtime API, or the Cloudflare Workers module shape) before the method-scan even runs, closing the standalone-repo blind spot (fixtures below) while keeping the FE/extension cases out (also below).

#[test]
fn fetch_without_timeout_in_a_standalone_worker_repo_via_default_export_shape_is_flagged() {
    // Standalone-BE-repo shape (no be/api/server path segment at all) — a Cloudflare-Worker-style file recognized via the `export default { ... }` module-worker shape. The parser does not project a `SourceSymbol` body span for a method *shorthand* inside an object literal (`scheduled(...) {}`), so the fetch itself lives in an ordinary named helper function that the worker's `scheduled` handler calls — the `export default {` signal in the file is what lets method-scan reach it.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/jobs/refresh.ts",
        "declare const ECB_URL: string;\n\nasync function fetchRates() {\n  return fetch(ECB_URL);\n}\n\nexport default {\n  async scheduled(controller: any, env: any, ctx: any) {\n    return fetchRates();\n  },\n};\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "fetch-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn fetch_without_timeout_in_a_standalone_worker_repo_referencing_d1_database_is_flagged() {
    // Same standalone-BE-repo shape, recognized instead via a Cloudflare Workers binding type (`D1Database`) referenced anywhere in the file.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/jobs/refresh.ts",
        "interface Env {\n  DB: D1Database;\n}\n\nexport async function refresh(env: Env) {\n  const res = await fetch(\"https://api.example.com/rates\");\n  return res.json();\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "fetch-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

#[test]
fn fetch_without_timeout_in_a_standalone_worker_repo_via_d1_prepare_call_is_flagged() {
    // A Workers cron/`scheduled`-handler helper can import only the `Env` *type* — the `export default { ... }` shape and the `D1Database` type name may live in a sibling `index.ts`/`types.ts`, invisible to this per-file `require_file` gate — while doing an untimed `fetch` followed by `env.DB.prepare(...)`.
    // None of the other alternatives (`D1Database`/`KVNamespace`/`ExecutionContext`/`export default {`) appear anywhere in this file, so without the `.prepare(` signal the rule would miss it.
    // The D1 prepared-statement call itself is the strongest signal actually present in the fetch-bearing file, and it generalizes across whatever the D1 binding happens to be named (`env.DB`, `env.MY_DB`, `c.env.DB`, ...) rather than being tied to the literal name `DB`.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/refreshRates.ts",
        "import type { Env } from \"./types.js\";\n\nexport async function refreshRates(env: Env): Promise<boolean> {\n  const res = await fetch(\"https://example.com/rates.xml\");\n  if (!res.ok) return false;\n  await env.DB.prepare(\"INSERT INTO rates (id) VALUES (1)\").run();\n  return true;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "fetch-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn fetch_without_timeout_in_an_express_server_file_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/app.ts",
        "import express from \"express\";\n\nconst app = express();\n\nasync function getRates() {\n  const res = await fetch(\"https://api.example.com/rates\");\n  return res.json();\n}\n\napp.get(\"/rates\", getRates);\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "fetch-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

#[test]
fn fetch_in_a_frontend_react_component_with_no_server_signal_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "apps/web-fe/src/Data.tsx",
        "import { useEffect } from \"react\";\n\nexport function Data() {\n  useEffect(() => {\n    fetch(\"/api/data\").then((r) => r.json());\n  }, []);\n  return null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fetch-no-timeout").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn fetch_in_a_chrome_extension_background_script_with_no_server_signal_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "extension/background.ts",
        "export async function loadLocalResource() {\n  const res = await fetch(chrome.runtime.getURL(\"data.json\"));\n  return res.json();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fetch-no-timeout").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn fetch_with_abort_signal_timeout_in_an_express_file_is_not_flagged() {
    // Content-based scoping still combines with the existing absent-veto: the server signal (express import) gets the file past `require_file`, but the AbortSignal.timeout at the call site vetoes it.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/app.ts",
        "import express from \"express\";\n\nexport async function getRates() {\n  const res = await fetch(\"https://api.example.com/rates\", { signal: AbortSignal.timeout(5000) });\n  return res.json();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fetch-no-timeout").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn fetch_timeout_ok_marker_above_the_call_in_a_server_file_suppresses_the_finding() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/app.ts",
        "import express from \"express\";\n\nexport async function getRates() {\n  // fetch-timeout-ok: axios instance configured with a global timeout in http-client.ts\n  const res = await fetch(\"https://api.example.com/rates\");\n  return res.json();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fetch-no-timeout").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- process-exit-in-lib ---

#[test]
fn process_exit_inside_a_function_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/shutdown.ts",
        "export function shutdown(reason: string) {\n  console.error(reason);\n  process.exit(1);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "process-exit-in-lib");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn process_exit_inside_a_scripts_dir_cli_file_is_not_flagged() {
    // process.exit is the expected/idiomatic way for a CLI entrypoint to exit, so scripts/**.cjs files are excluded outright rather than flagged.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "scripts/build.cjs",
        "function main(code) {\n  console.error('build failed');\n  process.exit(code);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "process-exit-in-lib").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_exit_at_module_top_level_is_not_scanned() {
    // Same method-scan precision limit as `async-route-no-catch`: no enclosing function body -> no symbol span -> method-scan silently skips it.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/cli.ts",
        "declare const reason: string;\nprocess.exit(1);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "process-exit-in-lib").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_exit_inside_a_sigterm_handler_in_a_signal_handling_module_is_not_flagged() {
    // A canonical graceful-shutdown module — process.exit(...) called from inside a process.on('SIGTERM', ...) handler is the idiomatic pattern, not a library-code bug.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/shutdown.ts",
        "export function registerShutdown() {\n  process.on('SIGTERM', () => {\n    process.exit(0);\n  });\n  process.on('SIGINT', () => {\n    process.exit(0);\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "process-exit-in-lib").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_exit_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/cli.ts",
        "export function main(code: number) {\n  // process-exit-ok: this is the CLI entrypoint, exiting here is intentional\n  process.exit(code);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "process-exit-in-lib").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- body-limit-missing ---

#[test]
fn express_json_without_limit_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write("src/app.ts", "app.use(express.json());\n");
    let out = scan(&dir);
    let h = hits(&out, "body-limit-missing");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn express_json_with_explicit_limit_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write("src/app.ts", "app.use(express.json({ limit: '1mb' }));\n");
    let out = scan(&dir);
    assert!(
        hits(&out, "body-limit-missing").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn body_limit_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/app.ts",
        "// body-limit-ok: internal admin endpoint, payload size bounded upstream by the LB\napp.use(bodyParser.json());\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "body-limit-missing").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- console-in-be ---

#[test]
fn console_log_under_api_directory_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write("src/api/handler.ts", "console.log(\"hit\");\n");
    let out = scan(&dir);
    let h = hits(&out, "console-in-be");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn console_log_outside_backend_directories_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write("src/utils/logger.ts", "console.log(\"hit\");\n");
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

#[test]
fn console_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/api/handler.ts",
        "// console-ok: temporary trace, removed before merge\nconsole.log(\"hit\");\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

// --- interval-no-clear ---

#[test]
fn set_interval_without_any_clear_interval_in_file_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/poller.ts",
        "export function startPolling() {\n  const id = setInterval(() => {\n    console.log(\"tick\");\n  }, 1000);\n  return id;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "interval-no-clear");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn set_interval_with_a_clear_interval_elsewhere_in_the_same_file_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/poller.ts",
        "export function startPolling() {\n  const id = setInterval(() => {\n    console.log(\"tick\");\n  }, 1000);\n  return id;\n}\nexport function stopPolling(id: any) {\n  clearInterval(id);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "interval-no-clear").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn interval_ok_marker_above_the_set_interval_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/poller.ts",
        "export function startPolling() {\n  // interval-ok: cleared by the host process's own lifecycle hook\n  const id = setInterval(() => {\n    console.log(\"tick\");\n  }, 1000);\n  return id;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "interval-no-clear").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- env-outside-config ---

#[test]
fn process_env_access_outside_a_config_module_is_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/handler.ts",
        "export function getPort() {\n  return process.env.PORT;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "env-outside-config");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn process_env_access_inside_a_config_file_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write("src/config.ts", "export const port = process.env.PORT;\n");
    let out = scan(&dir);
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_env_access_under_a_config_directory_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/config/database.ts",
        "export const dbUrl = process.env.DATABASE_URL;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_env_access_in_a_dot_config_suffix_file_is_not_flagged() {
    // `*.config.*`-suffix build-tool entrypoints like `next.config.mjs` are a naming convention the basename-STARTS-WITH-`config`/`env` and `config/`/`settings/`-directory checks alone don't cover, since `next.config.mjs` neither starts with `config` nor lives under a `config/` directory.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "apps/web/next.config.mjs",
        "export default { env: { API_URL: process.env.API_URL } };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_env_access_in_a_dot_config_ts_suffix_file_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "playwright.config.ts",
        "export default { use: { baseURL: process.env.BASE_URL } };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_env_access_in_a_dotfile_rc_config_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        ".eslintrc.js",
        "module.exports = { rules: process.env.STRICT ? {} : {} };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_env_access_in_a_regular_src_file_still_fires_alongside_the_new_exemptions() {
    // Regression guard for the config-suffix/rc-suffix exemptions — a plain src file must still fire; only those specific shapes are exempt.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "apps/web/src/analytics.ts",
        "export function track() {\n  return process.env.ANALYTICS_KEY;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "env-outside-config");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn process_env_access_in_a_constants_module_is_not_flagged() {
    // packages/lib/constants.ts (and per-package */lib/constants.ts files) are a common JS-monorepo convention for a package's config module, so the basename exemption covers `constants*` alongside `config*`/`env*`.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "packages/lib/constants.ts",
        "export const WEBAPP_URL = process.env.NEXT_PUBLIC_WEBAPP_URL;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_env_access_in_a_non_constants_file_still_fires() {
    // Regression guard: `constants*` is a basename exemption, not a blanket "anything under lib/" pass.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "packages/lib/server/session.ts",
        "export function getSecret() {\n  return process.env.SESSION_SECRET;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "env-outside-config");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn process_env_access_in_a_test_fixture_path_is_not_flagged() {
    // env-outside-config is a code-organization convention rule, not a security rule, so a test fixture reading process.env directly isn't the scattering this rule targets.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/handler.test.ts",
        "it('reads a var', () => {\n  expect(process.env.PORT).toBeDefined();\n});\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_env_access_in_a_scripts_dir_is_not_flagged() {
    // A one-off seed/migration script reading env directly is fine — this rule is about scattering across application code, not about every process.env read in the repo.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "scripts/seed.ts",
        "async function seed() {\n  const url = process.env.DATABASE_URL;\n  console.log(url);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn env_access_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/handler.ts",
        "export function getPort() {\n  // env-access-ok: legacy call site, migration tracked in JIRA-123\n  return process.env.PORT;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_env_nonnull_assertion_outside_config_fires_both_env_rules_on_the_same_line() {
    // Documented interplay (be-reliability.json's env-outside-config message): env-nonnull-assert (deferred-crash risk of `!`) and env-outside-config (scattered env access) are different concerns, so both firing on the same line is intended, not a duplicate.
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/handler.ts",
        "export const key = process.env.API_KEY!;\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "env-nonnull-assert").len(),
        1,
        "{:?}",
        out.findings
    );
    assert_eq!(
        hits(&out, "env-outside-config").len(),
        1,
        "{:?}",
        out.findings
    );
}

// --- skip_comment_lines + test-path file_exclude_pattern ---
// Every deployed-surface rule in this pack enables `skip_comment_lines` (a commented-out example of a flagged shape must not fire) and shares the test-path `file_exclude_pattern` (same string as `debug-true-committed`, already exercised above).

#[test]
fn async_route_shape_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/routes.ts",
        "export function registerRoutes(app: any) {\n  // app.get(\"/items\", async (req, res) => { ... }) -- old handler, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "async-route-no-catch").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn json_parse_of_request_body_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zpz-be-rel");
    dir.write(
        "src/__tests__/handler.test.ts",
        "export function handleBody(req: any) {\n  const parsed = JSON.parse(req.body);\n  return parsed;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "json-parse-no-try").is_empty(),
        "{:?}",
        out.findings
    );
}
