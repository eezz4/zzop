//! End-to-end coverage for the three fullstack io/graph native rules added alongside `duplicate-route`/
//! `route-shadowing`'s siblings (rule-pack catalog #46/#47/#48): `route-shadowing`, `mutating-route-no-auth`,
//! and `unprovided-consume` — all `zzop_rules_graph`, wired into `zzop_engine::analyze::assemble` beside
//! `schema-usage`/`duplicate-route`. Same `TempDir` fixture-tree pattern as `pack_sql.rs`/
//! `analyze_callgraph.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RuleConfig;
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

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

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings.iter().filter(|f| f.rule_id == rule).collect()
}

fn scan_with(dir: &TempDir, rule_config: RuleConfig) -> AnalyzeOutput {
    analyze_tree(
        dir.path(),
        &EngineConfig {
            rule_config,
            ..EngineConfig::default()
        },
    )
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    scan_with(dir, RuleConfig::default())
}

fn disabled(rule: &str) -> RuleConfig {
    RuleConfig {
        disabled_rules: vec![rule.to_string()],
        ..RuleConfig::default()
    }
}

// ---------------------------------------------------------------------------------------------
// route-shadowing (#46)
// ---------------------------------------------------------------------------------------------

/// An earlier `:id` param route shadows a later `/items/active` literal route in the same file.
fn route_shadowing_fixture() -> TempDir {
    let dir = TempDir::new("zzop-route-shadowing");
    dir.write(
        "routes/items.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/items/:id\", api.getItem);\napiRoutes.get(\"/items/active\", api.listActive);\n",
    );
    dir
}

#[test]
fn earlier_param_route_shadowing_a_later_literal_route_is_flagged() {
    let dir = route_shadowing_fixture();
    let out = scan(&dir);
    let found = hits(&out, "route-shadowing");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "routes/items.ts");
    assert_eq!(found[0].line, 3);
    assert_eq!(found[0].severity, zzop_core::Severity::Warning);
    assert!(found[0].message.contains("line 2"));
    assert!(found[0].message.contains("disabled_rules"));
}

#[test]
fn literal_route_registered_before_the_param_route_is_not_flagged() {
    let dir = TempDir::new("zzop-route-shadowing-negative");
    dir.write(
        "routes/items.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/items/active\", api.listActive);\napiRoutes.get(\"/items/:id\", api.getItem);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-shadowing").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn route_shadowing_disabled_via_config_removes_the_finding() {
    let dir = route_shadowing_fixture();
    let out = scan_with(&dir, disabled("route-shadowing"));
    assert!(
        hits(&out, "route-shadowing").is_empty(),
        "{:?}",
        out.findings
    );
}

// ---------------------------------------------------------------------------------------------
// mutating-route-no-auth (#47)
// ---------------------------------------------------------------------------------------------

/// `POST /users` -> `createUser` never calls anything guard-shaped. `DELETE /users/:id` ->
/// `deleteUserGuarded` calls `requireAuth` (same-file call edge) before deleting.
fn mutating_no_auth_fixture() -> TempDir {
    let dir = TempDir::new("zzop-mutating-no-auth");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/users\", createUser);\napiRoutes.delete(\"/users/:id\", deleteUserGuarded);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function createUser(c) {\n  return prisma.user.create({ data: {} });\n}\n\nexport function deleteUserGuarded(c) {\n  requireAuth(c);\n  return prisma.user.delete({ where: { id: c.id } });\n}\n\nexport function requireAuth(c) {\n  return true;\n}\n",
    );
    dir
}

#[test]
fn mutating_handler_never_reaching_a_guard_is_flagged() {
    let dir = mutating_no_auth_fixture();
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "routes/api.ts");
    assert_eq!(found[0].line, 2);
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
    let data = found[0].data.as_ref().unwrap();
    assert_eq!(data["method"], "POST");
    assert_eq!(data["path"], "/users");
    assert!(found[0].message.contains("disabled_rules"));
    // Never fires for the guarded DELETE handler.
    assert!(!found.iter().any(|f| f.line == 3));
}

#[test]
fn auth_acquisition_path_route_with_no_guard_is_never_flagged() {
    let dir = TempDir::new("zzop-mutating-no-auth-acquisition");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/api/auth/register\", registerUser);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function registerUser(c) {\n  return prisma.user.create({ data: {} });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn conditional_exempt_segment_with_no_auth_family_segment_still_fires() {
    // /devices/register — "register" is a conditional-tier auth-acquisition word, but no auth-family
    // segment (auth/login/signin/signup/session/oauth) appears anywhere else in the path, so this is an
    // ordinary device-registration route, not the auth-acquisition surface — it is checked normally.
    let dir = TempDir::new("zzop-mutating-no-auth-devices-register");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/devices/register\", registerDevice);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function registerDevice(c) {\n  return prisma.device.create({ data: {} });\n}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].data.as_ref().unwrap()["path"], "/devices/register");
}

#[test]
fn conditional_exempt_segment_paired_with_auth_family_segment_is_exempt() {
    // /auth/register — "register" (conditional tier) paired with "auth" (auth-family) elsewhere in the
    // path IS exempt, unlike the standalone /devices/register case above.
    let dir = TempDir::new("zzop-mutating-no-auth-auth-register");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/auth/register\", registerAccount);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function registerAccount(c) {\n  return prisma.user.create({ data: {} });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn author_path_route_with_no_guard_still_fires_segment_precision() {
    // Handler is deliberately named without an "auth" substring (unlike the path) so this test isolates
    // the PATH-segment exemption from the separate, unrelated guard-vocabulary name match on the handler
    // symbol itself (which would independently clear a handler literally named e.g. `updateAuthorProfile`).
    let dir = TempDir::new("zzop-mutating-no-auth-author");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/author/profile\", patchWriterBio);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function patchWriterBio(c) {\n  return prisma.author.update({ where: { id: c.id }, data: {} });\n}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
}

#[test]
fn handler_reaching_a_guard_looking_callee_is_not_flagged() {
    let dir = mutating_no_auth_fixture();
    let out = scan(&dir);
    assert!(
        !hits(&out, "mutating-route-no-auth")
            .iter()
            .any(|f| f.data.as_ref().unwrap()["path"] == "/users/{}"),
        "{:?}",
        out.findings
    );
}

#[test]
fn mutating_route_no_auth_disabled_via_config_removes_the_finding() {
    let dir = mutating_no_auth_fixture();
    let out = scan_with(&dir, disabled("mutating-route-no-auth"));
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mutating_route_registered_in_a_test_fixture_file_is_not_flagged() {
    // A route defined/invoked only from a __test__/__tests__ fixture file (to exercise a handler in
    // isolation) is not exposed application surface, and must not be flagged.
    let dir = TempDir::new("zzop-mutating-no-auth-test-fixture");
    dir.write(
        "routes/__tests__/api.test.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/users\", createUser);\n",
    );
    dir.write(
        "routes/__tests__/handlers.test.ts",
        "export function createUser(c) {\n  return prisma.user.create({ data: {} });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn nestjs_class_level_use_guards_exempts_every_route_in_the_controller() {
    // A class-level `@UseGuards` chain genuinely guards every route in the controller, but the
    // handler's own body never calls anything guard-named, so the call-graph BFS alone would
    // false-positive on it. This is the end-to-end test proving the fix works through the real
    // engine: real swc parse, real BFS, real `extract_nest_guarded_lines` wiring in `analyze.rs`.
    let dir = TempDir::new("zzop-mutating-no-auth-nest-guarded");
    dir.write(
        "items.controller.ts",
        concat!(
            "import { Controller, Post, UseGuards } from '@nestjs/common';\n\n",
            "@Controller('items')\n",
            "@UseGuards(JwtAuthGuard)\n",
            "export class ItemsController {\n",
            "  @Post('x')\n",
            "  async create() {\n",
            "    return true;\n",
            "  }\n",
            "}\n"
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn nestjs_route_with_no_use_guards_anywhere_still_fires() {
    // Companion negative fixture — proves the NestJS `@UseGuards` exemption is precise, not a blanket
    // suppression of the whole rule for NestJS-shaped trees.
    let dir = TempDir::new("zzop-mutating-no-auth-nest-unguarded");
    dir.write(
        "items.controller.ts",
        concat!(
            "import { Controller, Post } from '@nestjs/common';\n\n",
            "@Controller('items')\n",
            "export class ItemsController {\n",
            "  @Post('x')\n",
            "  async create() {\n",
            "    return true;\n",
            "  }\n",
            "}\n"
        ),
    );
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "items.controller.ts");
}

#[test]
fn safe_get_routes_are_never_checked_for_missing_auth() {
    let dir = TempDir::new("zzop-mutating-no-auth-get");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/users\", listUsers);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function listUsers(c) {\n  return prisma.user.findMany();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

// ---------------------------------------------------------------------------------------------
// unprovided-consume (#48)
// ---------------------------------------------------------------------------------------------

/// This tree both provides `GET /api/users` and consumes it (matched, line 2) plus a nonexistent
/// `GET /api/missing` (unmatched, line 3).
fn unprovided_consume_fixture() -> TempDir {
    let dir = TempDir::new("zzop-unprovided-consume");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.listUsers);\n",
    );
    dir.write(
        "src/client.ts",
        "export function loadUsers() { return axios.get(\"/api/users\"); }\nexport function loadMissing() { return axios.get(\"/api/missing\"); }\n",
    );
    dir
}

#[test]
fn unmatched_consume_is_flagged_when_the_tree_also_provides_http_routes() {
    let dir = unprovided_consume_fixture();
    let out = scan(&dir);
    let found = hits(&out, "unprovided-consume");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "src/client.ts");
    assert_eq!(found[0].line, 2);
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
    assert_eq!(
        found[0].data.as_ref().unwrap()["key"].as_str(),
        Some("GET /api/missing")
    );
    assert!(found[0].message.contains("disabled_rules"));
    // The matched consume (line 1) never fires.
    assert!(!found.iter().any(|f| f.line == 1));
}

#[test]
fn static_asset_extension_consume_is_never_flagged() {
    let dir = TempDir::new("zzop-unprovided-consume-asset");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.listUsers);\n",
    );
    dir.write(
        "src/client.ts",
        "export function loadConfig() { return axios.get(\"/public/config.json\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unprovided-consume").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn rails_style_json_api_route_with_an_api_segment_still_fires() {
    // GET /api/users.json is a real Rails-style, format-suffixed API route — the /api/ segment stops
    // the default json/xml veto from applying.
    let dir = TempDir::new("zzop-unprovided-consume-json-api");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/accounts\", api.listAccounts);\n",
    );
    dir.write(
        "src/client.ts",
        "export function loadUsers() { return axios.get(\"/api/users.json\"); }\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "unprovided-consume");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(
        found[0].data.as_ref().unwrap()["key"].as_str(),
        Some("GET /api/users.json")
    );
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
}

#[test]
fn json_under_a_public_asset_directory_is_vetoed() {
    let dir = TempDir::new("zzop-unprovided-consume-public-json");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.listUsers);\n",
    );
    dir.write(
        "src/client.ts",
        "export function loadRecipes() { return axios.get(\"/public/recipes.json\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unprovided-consume").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn next_js_public_prefix_stripped_json_path_is_vetoed() {
    // Next.js serves public/ files with the `public/` prefix stripped from the URL, so
    // `public/i18n/ko.json` on disk is fetched at `GET /i18n/ko.json` — no asset-directory segment
    // survives in the key for a directory allowlist to match. The inverted (API-segment) gate still
    // vetoes it since no /api/,/graphql/,/rpc/,/vN/ segment is present either.
    let dir = TempDir::new("zzop-unprovided-consume-i18n-json");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.listUsers);\n",
    );
    dir.write(
        "src/client.ts",
        "export function loadLocale() { return axios.get(\"/i18n/ko.json\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unprovided-consume").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pure_fe_tree_with_zero_http_provides_is_never_flagged() {
    let dir = TempDir::new("zzop-unprovided-consume-pure-fe");
    dir.write(
        "src/client.ts",
        "export function loadRemote() { return axios.get(\"/remote/thing\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unprovided-consume").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn unprovided_consume_disabled_via_config_removes_the_finding() {
    let dir = unprovided_consume_fixture();
    let out = scan_with(&dir, disabled("unprovided-consume"));
    assert!(
        hits(&out, "unprovided-consume").is_empty(),
        "{:?}",
        out.findings
    );
}
