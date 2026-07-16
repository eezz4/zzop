use crate::{hits, scan, TempDir};

// --- async-route-no-catch ---

#[test]
fn async_route_without_try_catch_or_next_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/handler.ts",
        "import { readFileSync } from \"fs\";\nexport function handler(req: any, res: any) {\n  const data = readFileSync(\"./config.json\", \"utf8\");\n  res.status(200).json(data);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sync-fs-in-handler");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn sync_read_file_in_module_init_with_no_handler_context_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
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
fn sync_fs_in_a_build_script_is_not_flagged() {
    // Class fix (mono-hub 0.10.0 FP): a build-time data-collection script under scripts/ names its
    // fetch response `const res = await fetch(...)`, which the `res` handler-context token matched even
    // though it's not an Express handler. Build/CLI/tooling paths (scripts/tools/bin) are off the
    // request path — sync fs there is fine — so they're excluded, mirroring `process-exit-in-lib`.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "scripts/fetch-data.cjs",
        "async function main() {\n  const res = await fetch(\"https://example.com/data\");\n  const json = await res.json();\n  writeFileSync(\"./out.json\", JSON.stringify(json));\n}\n",
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
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/handler.ts",
        "import { readFileSync } from \"fs\";\nexport function handler(req: any, res: any) {\n  // sync-io-ok: startup-time cache warm, not on the per-request path\n  const data = readFileSync(\"./config.json\", \"utf8\");\n  res.status(200).json(data);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sync-fs-in-handler").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn fetch_response_variable_named_res_is_not_handler_context_evidence_outside_scripts_dir() {
    // Field FP fix (mono-hub 0.10.0): a bare `res` identifier bound to a fetch Response satisfied the old
    // naive handler-context vocabulary (`\b(req|res|ctx|request|reply)\b`), so a data-fetching helper named
    // `res` looked like an Express handler even outside scripts/. The new evidence requires a `req.`/
    // `request.` member access or a response-API CALL fetch's own Response shape doesn't have
    // (`res.json(`/`res.text(` are deliberately excluded — see the rule's message), so this stays silent
    // without relying on the scripts/ path exclusion.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/jobs/refresh.ts",
        "import { writeFileSync } from \"fs\";\nexport async function refresh() {\n  const res = await fetch(\"https://example.com/data\");\n  const data = await res.json();\n  writeFileSync(\"./out.json\", JSON.stringify(data));\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sync-fs-in-handler").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_with_only_bare_res_json_evidence_is_now_silent_accepted_false_negative() {
    // Documented limitation (see be-reliability.json's sync-fs-in-handler message): `res.json(x)` alone no
    // longer counts as handler-context evidence — it's deliberately excluded because a fetch `Response` also
    // has `.json()`, which is exactly the false-positive class the vocabulary swap above fixes. A handler
    // whose ONLY evidence is `res.json(...)` (no `req.`/`request.` member access, no other response-API call)
    // is now an accepted false negative — pinned here so the tradeoff can't silently drift.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/handler.ts",
        "import { readFileSync } from \"fs\";\nexport function handler(req: any, res: any) {\n  const data = readFileSync(\"./config.json\", \"utf8\");\n  res.json(data);\n}\n",
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/process.ts",
        "declare function fetchItem(id: string): Promise<unknown>;\nexport async function process(ids: string[]) {\n  return Promise.all(ids.map(async (id) => {\n    return await fetchItem(id);\n  }));\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "await-in-map").is_empty(), "{:?}", out.findings);
}

#[test]
fn map_async_ok_marker_above_the_map_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/process.ts",
        "declare function notify(id: string): Promise<unknown>;\nexport async function process(ids: string[]) {\n  // map-async-ok: fire-and-forget notifications, failures logged elsewhere\n  return ids.map(async (id) => {\n    return await notify(id);\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "await-in-map").is_empty(), "{:?}", out.findings);
}
