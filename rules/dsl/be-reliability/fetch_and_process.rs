use crate::{hits, scan, TempDir};

// --- fetch-no-timeout ---
//
// Scoping by backend-looking PATH alone would fire on every bare frontend fetch (and on a chrome.runtime.getURL local-resource call) while still missing a standalone backend repo rooted at `src/` with no `be`/`api`/`server`-ish path segment.
// Instead this rule gates on file CONTENT via `require_file` — the file text must show a server-side signal (framework import, server-runtime API, or the Cloudflare Workers module shape) before the method-scan even runs, closing the standalone-repo blind spot (fixtures below) while keeping the FE/extension cases out (also below).

#[test]
fn fetch_without_timeout_in_a_standalone_worker_repo_via_default_export_shape_is_flagged() {
    // Standalone-BE-repo shape (no be/api/server path segment at all) — a Cloudflare-Worker-style file recognized via the `export default { ... }` module-worker shape. The parser does not project a `SourceSymbol` body span for a method *shorthand* inside an object literal (`scheduled(...) {}`), so the fetch itself lives in an ordinary named helper function that the worker's `scheduled` handler calls — the `export default {` signal in the file is what lets method-scan reach it.
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
fn process_exit_inside_a_string_literal_is_not_flagged() {
    // A code-generation template or example emits the TEXT `process.exit(2)` as a string literal — it is
    // not a real call in THIS file. With `strip_string_literals`, the masked line no longer matches the
    // `process.exit(` pattern, so the code-gen helper is not falsely flagged. (Regression: the raw
    // per-line regex used to fire on the string's contents.)
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/codegen.ts",
        "export function emitExit(): string {\n  return 'if (err) { process.exit(2); }';\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "process-exit-in-lib").is_empty(),
        "a process.exit inside a string literal must not fire: {:?}",
        out.findings
    );
}

#[test]
fn a_real_call_on_the_same_line_as_a_string_literal_still_fires() {
    // The mask only blanks string INTERIORS — a genuine call outside the string on the same line is still
    // seen. Proves the masking doesn't over-suppress.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/shutdown.ts",
        "export function shutdown() {\n  logger.info('shutting down'); process.exit(1);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "process-exit-in-lib");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn process_exit_inside_a_scripts_dir_cli_file_is_not_flagged() {
    // process.exit is the expected/idiomatic way for a CLI entrypoint to exit, so scripts/**.cjs files are excluded outright rather than flagged.
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
