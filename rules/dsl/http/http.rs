//! Exercises `rules/dsl/http/http.json`'s HTTP-route rules end-to-end via `zpz_engine::analyze_tree` against
//! real swc-parsed TypeScript fixtures. See `http.json` for each rule's exact matcher shape and message.
//!
//! Ordering-aware and graph-shaped route checks (auth-state-machine transitions, API churn, unsafe-read-endpoint,
//! non-idempotent-write, FE/BE spec drift) are out of scope for a per-file DSL matcher and stay on the native-analysis backlog.
//!
//! All three rules require file paths shaped like `HTTP_SCANNER_DEFAULTS.beHandlerPathPattern`; fixtures
//! below use `src/routes/apiRoutes.ts` so the `/routes/` alternative matches. `/routes/` and `/controllers/`
//! require a slash on both sides, so a route file at the tree root with no parent directory would not match.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zpz_core::{load_dsl_packs, RulePackDef};
use zpz_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — same pattern as `sql/sql.rs`/`typescript/typescript.rs`).
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

/// Loads the real `http.json` pack, filtered so this test is unaffected by sibling packs under concurrent development.
fn http_pack() -> RulePackDef {
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
        .find(|p| p.id == "http")
        .expect("http pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "http-fixture".to_string(),
        packs: vec![http_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zpz_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("http/{rule}"))
        .collect()
}

// --- read-model-path ---

#[test]
fn get_endpoint_with_no_cache_marker_is_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\napiRoutes.get(\"/api/items\", api.itemList);\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "read-model-path").len(), 1, "{:?}", out.findings);
}

#[test]
fn cache_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\napiRoutes.get(\"/api/items\", api.itemList); // cache: getCachedList (cache:list:items)\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "read-model-path").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn no_cache_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\napiRoutes.get(\"/api/items/:id\", api.itemDetail); // no-cache: per-user state\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "read-model-path").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn post_endpoint_is_not_inspected() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\napiRoutes.post(\"/api/items\", api.itemCreate);\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "read-model-path").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn read_model_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\napiRoutes.get(\"/api/items\", api.itemList); // read-model-ok: legacy endpoint, cache handled at CDN layer\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "read-model-path").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn read_model_ok_marker_one_line_before_suppresses_the_finding() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\n// read-model-ok: static data served at edge, no server cache needed\napiRoutes.get(\"/api/items/:id\", api.itemDetail);\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "read-model-path").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn read_model_ok_marker_two_lines_before_does_not_suppress() {
    // `MARKER_LOOKBACK_LINES` = 1 (see the const's doc in `zpz_core::dsl`): a marker 2 lines above the
    // reported line is out of range and does not suppress the finding.
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\n// read-model-ok: static data, no cache needed\n// line 2\napiRoutes.get(\"/api/feed\", api.feed);\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "read-model-path").len(), 1, "{:?}", out.findings);
}

#[test]
fn read_model_ok_marker_four_lines_before_does_not_suppress() {
    // Further out-of-range boundary check, same contract as the 2-lines-above case above.
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\n// read-model-ok: this is too far above\n// line 2\n// line 3\n// line 4\napiRoutes.get(\"/api/feed\", api.feed);\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "read-model-path").len(), 1, "{:?}", out.findings);
}

// --- auth-gates ---

#[test]
fn admin_path_with_no_role_check_handler_is_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/users\", api.userList);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn internal_path_with_no_role_check_handler_is_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.post(\"/api/internal/metrics\", api.metricsWrite);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn dev_path_with_no_role_check_handler_is_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", api.devConfig);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn multiple_protected_paths_all_missing_auth_are_all_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/items\", api.itemList);\napiRoutes.delete(\"/api/internal/cache\", api.clearCache);\napiRoutes.get(\"/api/dev/flags\", api.featureFlags);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 3, "{:?}", out.findings);
}

#[test]
fn extra_path_segments_after_protected_segment_is_still_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/users/:id/detail\", api.userDetail);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_admin_keyword_passes() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const adminHandlers: any;\napiRoutes.get(\"/api/admin/users\", adminHandlers.userList);\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_role_keyword_passes() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const handlers: any;\napiRoutes.get(\"/api/internal/report\", handlers.roleBasedReport);\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_require_admin_call_passes() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const handlers: any;\ndeclare function requireAdmin(h: any): any;\napiRoutes.get(\"/api/admin/settings\", requireAdmin(handlers.settings));\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_guard_keyword_passes() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const guardedHandlers: any;\napiRoutes.delete(\"/api/internal/flush\", guardedHandlers.flush);\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn ordinary_path_with_no_protected_segment_is_not_inspected() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/users\", api.userList);\napiRoutes.post(\"/api/items\", api.itemCreate);\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_is_local_env_gate_hint_passes() {
    // An env-scoped gate like `if (!CONFIG.isLocal()) return 403;` must not be flagged as missing auth — an environment check does gate the route.
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const isLocalGuardedHandlers: any;\napiRoutes.get(\"/api/admin/users\", isLocalGuardedHandlers.userList);\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_node_env_hint_passes() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare function nodeEnvGuard(h: any): any;\napiRoutes.post(\"/api/internal/metrics\", nodeEnvGuard(handlers));\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn auth_gate_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/users\", api.userList); // auth-gate-ok: reviewed, gated at the API gateway layer\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

// --- route-exposure ---

#[test]
fn dev_path_with_no_env_guard_is_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", api.configHandler);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn debug_path_with_no_env_guard_is_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/debug/state\", api.stateSnapshot);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn internal_path_with_no_env_guard_is_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.post(\"/api/internal/flush\", api.cacheFlush);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn dunder_test_path_with_no_env_guard_is_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.delete(\"/api/__test__/reset\", api.seedReset);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn playground_path_with_no_env_guard_is_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/playground/echo\", api.echoHandler);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn multiple_dangerous_paths_without_guard_are_all_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/flags\", api.flagList);\napiRoutes.get(\"/api/debug/heap\", api.heapSnapshot);\napiRoutes.post(\"/api/internal/rebuild\", api.rebuildIndex);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 3, "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_dev_hint_passes() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const devOnlyHandlers: any;\napiRoutes.get(\"/api/dev/config\", devOnlyHandlers.config);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_guard_hint_passes() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const guardedMetrics: any;\napiRoutes.get(\"/api/internal/metrics\", guardedMetrics.handler);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_require_dev_hint_passes() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare function requireDevAccess(): any;\napiRoutes.get(\"/api/debug/state\", requireDevAccess);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_is_production_hint_passes() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const isProductionGuarded: any;\napiRoutes.get(\"/api/dev/tools\", isProductionGuarded.tools);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_is_local_hint_passes() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare function isLocalOnlyEcho(): any;\napiRoutes.get(\"/api/playground/echo\", isLocalOnlyEcho);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn ordinary_paths_are_not_inspected() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/users\", api.userList);\napiRoutes.post(\"/api/items\", api.itemCreate);\napiRoutes.get(\"/api/health\", api.health);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_node_env_hint_passes_route_exposure() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare function nodeEnvGuard(h: any): any;\napiRoutes.get(\"/api/dev/tools\", nodeEnvGuard(handlers));\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn route_exposure_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", api.configHandler); // route-exposure-ok: reviewed, disabled outside CI\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- skip_comment_lines + test-path file_exclude_pattern ---
// A commented-out route registration must not fire any of these rules, and each excludes test-fixture
// paths (e.g. this pack's own `__tests__` dir) as scaffolding, not a deployed route.

#[test]
fn admin_route_shape_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\n// apiRoutes.get(\"/api/admin/users\", api.userList) -- moved below with a guard\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn dev_route_registered_in_a_routes_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zpz-http");
    dir.write(
        "src/routes/__tests__/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", api.configHandler);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}
