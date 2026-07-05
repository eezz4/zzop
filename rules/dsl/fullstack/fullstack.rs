//! End-to-end coverage for `rules/dsl/fullstack/fullstack.json` plus the native `duplicate-route` analysis (`zpz_engine::pipeline::duplicate_route_findings`, registered in `zpz_rules_graph::register_native_analyses`).
//!
//! `duplicate-route` is native, not a DSL rule, so its findings carry the plain id `"duplicate-route"` (no `fullstack/` prefix) despite running alongside the pack here.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zpz_core::{load_dsl_packs, RulePackDef};
use zpz_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — same pattern as `sql/sql.rs`/`http/http.rs`).
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

/// Loads the real `fullstack.json` pack, filtered so this test is unaffected by sibling packs under concurrent development.
fn fullstack_pack() -> RulePackDef {
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
        .find(|p| p.id == "fullstack")
        .expect("fullstack pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "fullstack-fixture".to_string(),
        packs: vec![fullstack_pack()],
        ..EngineConfig::default()
    }
}

fn config_with(rule_config: zpz_core::RuleConfig) -> EngineConfig {
    EngineConfig {
        source_id: "fullstack-fixture".to_string(),
        packs: vec![fullstack_pack()],
        rule_config,
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zpz_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("fullstack/{rule}"))
        .collect()
}

// --- mixed-content-egress ---

#[test]
fn plain_http_url_literal_is_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"http://example.com/api\"); }\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "mixed-content-egress");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 1);
}

#[test]
fn https_url_literal_is_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"https://example.com/api\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mixed-content-egress").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn xml_namespace_uri_is_excluded() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/xmlns.ts",
        "export const ns = \"http://www.w3.org/2000/svg\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mixed-content-egress").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mixed_content_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"http://example.com/api\"); } // mixed-content-ok\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mixed-content-egress").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- localhost-egress-committed ---

#[test]
fn committed_localhost_endpoint_is_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"http://localhost:3000/api\"); }\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "localhost-egress-committed");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 1);
}

#[test]
fn committed_private_ip_endpoint_is_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"https://192.168.1.10:8080/api\"); }\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "localhost-egress-committed");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
}

#[test]
fn public_host_endpoint_is_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"https://api.example.com/v1\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn localhost_endpoint_in_a_playwright_e2e_config_is_not_flagged() {
    // A localhost target committed in a playwright/e2e config or test fixture is the intended target there, not a leaked dev endpoint.
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "e2e/playwright.config.ts",
        "export default { use: { baseURL: \"http://localhost:3000\" } };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn root_level_playwright_config_is_not_flagged() {
    // A root-level playwright.config.ts (not under e2e/) is excluded by basename anywhere in the tree, not just by path.
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "playwright.config.ts",
        "export default { use: { baseURL: \"http://localhost:3000\" } };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn nested_vitest_config_is_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "packages/app/vitest.config.ts",
        "export default { test: { env: { API_URL: \"http://localhost:4000\" } } };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn localhost_endpoint_in_src_still_fires() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/config/api.ts",
        "export const apiBase = \"http://localhost:4000/api\";\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "localhost-egress-committed");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
}

#[test]
fn localhost_inside_a_jsdoc_style_block_comment_is_not_flagged() {
    // `skip_comment_lines` covers every continuation line of a `/** ... */` block comment (trimmed text starts with `*`); the quoted URL here exercises comment-skipping, not a line_pattern miss.
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "/**\n * See \"http://localhost:3000/api\" for local dev.\n */\nexport function load() {}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn localhost_inside_a_headerless_block_comment_continuation_line_still_fires() {
    // Documented residual gap: a block-comment continuation line with no leading `*` isn't recognized by `skip_comment_lines` (line-local heuristic, no block-comment-state tracking), so it still fires.
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "/*\nSee \"http://localhost:3000/api\" for local dev.\n*/\nexport function load() {}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "localhost-egress-committed");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 2);
}

#[test]
fn localhost_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"http://localhost:3000/api\"); } // localhost-ok\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- localhost-egress-committed: additional false-positive shapes ---

#[test]
fn env_override_fallback_is_not_flagged() {
    // An env-override fallback IS the recommended remedy this rule would otherwise ask for — already applied, not a leak.
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export const apiUrl = process.env.API_URL || \"http://localhost:3000\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn is_production_ternary_fallback_is_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export const apiUrl = IS_PRODUCTION ? prodUrl : \"http://localhost:3000\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn localhost_endpoint_with_no_env_fallback_still_fires() {
    // Regression guard: the env-override veto must not swallow a plain committed localhost literal with no env/operator co-occurrence on the same line.
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export const apiUrl = \"http://localhost:3000\";\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "localhost-egress-committed");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
}

#[test]
fn new_url_dummy_base_argument_is_not_flagged() {
    // `new URL(x, "http://localhost")` uses the second argument only as a dummy base to parse a relative path — never an egress target.
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/parse.ts",
        "export function toPath(x: string) { return new URL(x, \"http://localhost\").pathname; }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn equality_comparison_against_localhost_literal_is_not_flagged() {
    // The literal is a comparison sentinel here, not a call target.
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/guard.ts",
        "export function isDev(url: string) { return url !== \"http://localhost:3000\"; }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn nestjs_e2e_spec_file_is_not_flagged() {
    // NestJS convention `*.e2e-spec.ts` uses a `-spec.` hyphen separator, not the literal `.spec.` the base pattern requires.
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/app.e2e-spec.ts",
        "it('boots', async () => { await fetch(\"http://localhost:3000/health\"); });\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn packages_testing_helper_dir_is_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "packages/testing/server.ts",
        "export const testServerUrl = \"http://localhost:3000\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn vite_config_basename_is_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "vite.config.ts",
        "export default { server: { proxy: { \"/api\": \"http://localhost:3000\" } } };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localhost-egress-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- get-with-body ---

#[test]
fn get_request_with_body_in_the_same_function_is_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() {\n  return fetch(url, {\n    method: 'GET',\n    body: JSON.stringify(data),\n  });\n}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "get-with-body");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 4);
}

#[test]
fn get_request_without_body_is_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() {\n  return fetch(url, { method: 'GET' });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-with-body").is_empty(), "{:?}", out.findings);
}

#[test]
fn post_request_with_body_is_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function save() {\n  return fetch(url, {\n    method: 'POST',\n    body: JSON.stringify(data),\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-with-body").is_empty(), "{:?}", out.findings);
}

#[test]
fn get_body_ok_marker_above_the_body_line_suppresses_the_finding() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() {\n  return fetch(url, {\n    method: 'GET',\n    // get-body-ok: legacy proxy requires it, verified server-side\n    body: JSON.stringify(data),\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-with-body").is_empty(), "{:?}", out.findings);
}

// --- ws-no-auth ---

#[test]
fn websocket_opened_without_auth_material_is_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/socket.ts",
        "export function connect() {\n  return new WebSocket(\"wss://example.com/stream\");\n}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "ws-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 2);
}

#[test]
fn websocket_opened_with_token_in_the_same_function_is_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/socket.ts",
        "export function connect(token: string) {\n  return new WebSocket(`wss://example.com/stream?token=${token}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "ws-no-auth").is_empty(), "{:?}", out.findings);
}

#[test]
fn ws_auth_ok_marker_above_the_websocket_call_suppresses_the_finding() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/socket.ts",
        "export function connect() {\n  // ws-auth-ok: public read-only market-data feed, no auth by design\n  return new WebSocket(\"wss://example.com/stream\");\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "ws-no-auth").is_empty(), "{:?}", out.findings);
}

// --- duplicate-route (native) ---

fn duplicate_route_hits(out: &AnalyzeOutput) -> Vec<&zpz_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == "duplicate-route")
        .collect()
}

#[test]
fn same_route_registered_in_two_files_is_flagged_once_at_the_later_site() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/routes/a.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.userList);\n",
    );
    dir.write(
        "src/routes/b.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.userListAgain);\n",
    );
    let out = scan(&dir);
    let found = duplicate_route_hits(&out);
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    // Sites are sorted (file, line): "a.ts" < "b.ts", so b.ts is the later/duplicate site and a.ts is the canonical first site named in the message.
    assert_eq!(found[0].file, "src/routes/b.ts");
    assert_eq!(found[0].line, 2);
    assert!(found[0].message.contains("GET /api/users"));
    assert!(found[0].message.contains("src/routes/a.ts:2"));
}

#[test]
fn two_distinct_routes_across_files_are_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/routes/a.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.userList);\n",
    );
    dir.write(
        "src/routes/b.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/items\", api.itemList);\n",
    );
    let out = scan(&dir);
    assert!(duplicate_route_hits(&out).is_empty(), "{:?}", out.findings);
}

#[test]
fn duplicate_route_can_be_disabled_via_rule_config() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/routes/a.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.userList);\n",
    );
    dir.write(
        "src/routes/b.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.userListAgain);\n",
    );
    let rule_config = zpz_core::RuleConfig {
        disabled_rules: vec!["duplicate-route".to_string()],
        ..zpz_core::RuleConfig::default()
    };
    let out = analyze_tree(dir.path(), &config_with(rule_config));
    assert!(duplicate_route_hits(&out).is_empty(), "{:?}", out.findings);
}

// --- skip_comment_lines + test-path file_exclude_pattern ---
// A commented-out GET-with-body shape must not fire `get-with-body`; `mixed-content-egress` shares the same test-path `file_exclude_pattern` as `localhost-egress-committed`.

#[test]
fn get_with_body_shape_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() {\n  // fetch(url, { method: 'GET', body: JSON.stringify(data) }) -- old, fixed below\n  return fetch(url, { method: 'GET' });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-with-body").is_empty(), "{:?}", out.findings);
}

#[test]
fn plain_http_url_literal_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zpz-fullstack");
    dir.write(
        "src/__tests__/client.test.ts",
        "export function load() { return fetch(\"http://example.com/api\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mixed-content-egress").is_empty(),
        "{:?}",
        out.findings
    );
}
