//! `localhost-egress-committed` tests, including the additional false-positive shapes (split from `fullstack.rs`).

use super::*;

// --- localhost-egress-committed ---

#[test]
fn committed_localhost_endpoint_is_flagged() {
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
    let dir = TempDir::new("zzop-fullstack");
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
