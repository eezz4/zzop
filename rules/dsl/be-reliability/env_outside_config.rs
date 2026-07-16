use crate::{hits, scan, TempDir};

// --- env-outside-config ---

#[test]
fn process_env_access_outside_a_config_module_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
    let dir = TempDir::new("zzop-be-rel");
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
