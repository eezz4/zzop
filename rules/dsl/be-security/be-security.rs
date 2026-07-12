//! End-to-end tests for `rules/dsl/be-security/be-security.json` (38 backend-security rules), exercised via
//! `zzop_engine::analyze_tree` so `Matcher::MethodScan` rules run against real parser-derived
//! `SourceSymbol` body spans (TypeScript via swc), not hand-built spans. Each rule below has at least
//! one positive fixture (asserting finding count AND line number) and one realistic negative
//! (near-miss) fixture; a handful of cases also exercise `suppress_marker`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — same pattern as `sql/sql.rs`).
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

/// Loads the real `rules/dsl/be-security/be-security.json` from the repo, filtered to just the `be-security` pack
/// so this test is unaffected by sibling packs under concurrent development (same convention as
/// `http/http.rs`).
///
/// `CARGO_MANIFEST_DIR` is the `rules` crate root (`rules/Cargo.toml`), so `dsl/` is `rules/dsl` — this
/// pack's own `be-security.json` lives one level down, at `rules/dsl/be-security/be-security.json`.
fn be_security_pack() -> RulePackDef {
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
        .find(|p| p.id == "be-security")
        .expect("be-security pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "be-security-fixture".to_string(),
        packs: vec![be_security_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("be-security/{rule}"))
        .collect()
}

// --- hardcoded-secret ---

#[test]
fn assignment_shaped_secret_literal_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const apiKey = \"abcd1234efgh5678\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "hardcoded-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn known_aws_key_prefix_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/creds.ts",
        "export const key = \"AKIAABCDEFGHIJKLMNOP\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn secret_read_from_process_env_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const apiKey = process.env.API_KEY;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn secret_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "// secret-ok: rotated test-only fixture key, not a real credential\nexport const apiKey = \"abcd1234efgh5678\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn snake_case_java_client_secret_constant_is_flagged() {
    // `\b(secret|...)` requires a non-word char immediately before the keyword, but `_` is itself
    // a word character in regex `\b` semantics, so a SNAKE_CASE suffix like `CLIENT_SECRET` has no
    // boundary to match against. The value below is a synthetic placeholder in the Google
    // client-secret shape (not a real credential).
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/ServiceAuthen.java",
        "public class ServiceAuthen {\n    final String AUTHEN_GOOGLE_CLIENT_SECRET = \"GOCSPX-Ab1Cd2Ef3Gh4Ij5Kl6Mn7Qr8\";\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "hardcoded-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn snake_case_ts_api_key_constant_is_flagged() {
    // Same underscore-boundary fix, TypeScript side.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const SERVICE_API_KEY = \"abcd1234efgh5678\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn mock_prefixed_test_fixture_api_key_is_not_flagged() {
    // A test-fixture mock value (`"test-key"`) announces itself as a placeholder by shape.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/fixtures.ts",
        "export const mockConfig = { apiKey: \"test-key\" };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mock_dummy_fake_sample_prefixed_secrets_are_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/fixtures.ts",
        "export const a = { apiKey: \"mock-abcd1234\" };\nexport const b = { apiKey: \"dummy-abcd1234\" };\nexport const c = { apiKey: \"fake-abcd1234\" };\nexport const d = { apiKey: \"sample-abcd1234\" };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mock_word_in_non_dash_spellings_is_not_flagged() {
    // A mock/placeholder value can slip a dash-prefix-only veto (`test-`/`mock-`/...) when the mock
    // word isn't dash-delimited (e.g. `mock_token`, `whsec_test`).
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/fixtures.ts",
        concat!(
            "export const a = { token: \"mock_token\" };\n",
            "export const b = { password: \"MOCK_PASS123\" };\n",
            "export const c = { token: \"mock_token_123\" };\n",
            "export const d = { secret: \"whsec_test\" };\n",
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn value_equal_to_its_own_identifier_is_not_flagged() {
    // Sentinel constants whose value equals the assigned identifier name
    // (`refresh_token = "refresh_token"`, `INVALID_API_KEY = "INVALID_API_KEY"`) are names/error codes,
    // not secrets — approximated by value shape since this matcher can't compare capture groups.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/grant-types.ts",
        concat!(
            "export const refresh_token = \"refresh_token\";\n",
            "export const INVALID_API_KEY = \"INVALID_API_KEY\";\n",
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn vaguer_changeme_placeholder_without_a_recognized_mock_prefix_still_flagged() {
    // Not narrowed further: `changeme`-shaped placeholders have no recognized mock prefix and are
    // lexically indistinguishable from a real secret, so they intentionally stay flagged. A dash-joined
    // variant like `"changeme-please"` would instead match the letters-only, no-digits, dash-joined
    // sentinel shape (same family as `refresh-token`/`access-token`), so this fixture uses a dash-free
    // word-plus-digits value to test the actual decision under test: no entropy floor.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const apiKey = \"changeme12345\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn camel_case_mock_prefixed_token_is_not_flagged() {
    // `token: 'testAccessToken'` announces itself as a mock/placeholder by the "test" prefix, but a
    // mock-word veto whose right-hand boundary requires a delimiter/digit/quote/line-end immediately
    // after the mock word misses the camelCase continuation `A` (start of `AccessToken`) — the boundary
    // must also accept an uppercase letter right after the mock word.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/fixtures.ts",
        "export const accessOrWorkspaceAgnosticToken = { token: \"testAccessToken\", expiresAt: \"\" };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn lowercase_continuation_after_mock_word_does_not_over_broaden_the_veto() {
    // Guards against over-matching: the `(?-i:[A-Z])` boundary alternative is case-sensitive and only
    // accepts an uppercase letter right after the mock word, so a plain lowercase continuation like
    // "testimonial" must not gain the veto. This fixture is a real candidate (the `token` identifier
    // satisfies the `assignment` pattern, so the value does reach `exclude_pattern`) — "test" is
    // immediately followed by lowercase "i", which matches none of the boundary alternatives
    // (`[-_"'`]`, digit, `(?-i:[A-Z])`, or line-end), so the mock-word veto correctly does not engage
    // and the value stays flagged as a real secret.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const token = \"testimonial12345678\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn dash_delimited_sentinel_token_value_is_not_flagged() {
    // Dash-delimited multi-word lowercase tokens like `refresh-token`/`access-token`/`new-password`
    // are name/sentinel shapes identical in spirit to the excluded underscore-delimited ones
    // (`refresh_token = "refresh_token"`), just with dashes instead of underscores.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/fixtures.ts",
        concat!(
            "export const a = { refreshToken: { token: \"refresh-token\" } };\n",
            "export const b = { token: \"access-token\" };\n",
            "export const c = { password: \"new-password\" };\n",
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn dash_prefixed_random_looking_key_with_digits_is_still_flagged() {
    // Regression guard: the dash-sentinel veto only matches letters-only segments, so a genuinely
    // random-looking secret that happens to start with a recognized word + dash (digits and mixed case
    // breaking up the dash-joined run) must stay flagged, not get swept up by the veto meant for clean
    // dictionary-word placeholders like `sk-workspace-bound-secret`.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const secret = \"sk-a1B2c3D4e5F6g7H8i9J0\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn pascal_case_route_name_value_keyed_by_a_secret_shaped_identifier_is_not_flagged() {
    // A route-name registry can key route names by an enum member whose NAME carries a secret-shaped
    // suffix (`CHANGE_PASSWORD`, `FORGOT_PASSWORD`), but the VALUE is an unrelated PascalCase view
    // identifier, not a credential — same "value is a name/sentinel, not a secret" family as the
    // UPPER_SNAKE_CASE/lower_snake_case/dash-case shapes above, just PascalCase.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/constants/navigation.ts",
        concat!(
            "export enum VIEWS {\n",
            "  FORGOT_PASSWORD = 'ForgotMyPasswordView',\n",
            "  CHANGE_PASSWORD = 'ChangePasswordView',\n",
            "}\n"
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pascal_case_single_word_value_does_not_gain_the_multi_word_sentinel_veto() {
    // Regression guard: the PascalCase sentinel requires at least two capitalized segments (same
    // "multi-word" narrowness as the dash/underscore sentinel siblings) — a single PascalCase word is
    // not distinguishable from a real key that happens to be capitalized, so it must stay flagged.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const apiKey = \"Changemeplease\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

// --- mass-assignment ---

#[test]
fn req_body_passed_as_data_into_update_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/users.ts",
        "declare const prisma: any;\nexport async function updateUser(req: any) {\n  return prisma.user.update({ data: req.body });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "mass-assignment");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn req_body_spread_into_updatemany_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/users.ts",
        "declare const prisma: any;\nexport async function patchUsers(req: any) {\n  return prisma.user.updateMany({ ...req.body });\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "mass-assignment").len(), 1, "{:?}", out.findings);
}

#[test]
fn whitelisted_field_passed_into_create_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/items.ts",
        "declare const prisma: any;\nexport async function createItem(req: any) {\n  return prisma.item.create({ data: { name: req.body.name } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mass-assignment").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mass_assignment_ok_marker_above_the_write_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/users.ts",
        "declare const prisma: any;\nexport async function updateUser(req: any) {\n  // mass-assignment-ok: internal admin-only migration endpoint, body pre-validated upstream\n  return prisma.user.update({ data: req.body });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mass-assignment").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- raw-query-interpolation ---

#[test]
fn query_raw_unsafe_call_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/reports.ts",
        "declare const prisma: any;\ndeclare const id: string;\nexport async function f() {\n  return prisma.$queryRawUnsafe(`SELECT * FROM users WHERE id = ${id}`);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "raw-query-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn execute_raw_unsafe_call_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/admin.ts",
        "declare const prisma: any;\ndeclare const sql: string;\nexport async function f() {\n  return prisma.$executeRawUnsafe(sql);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "raw-query-interpolation").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn parameterized_execute_raw_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/admin.ts",
        "declare const prisma: any;\nexport async function f() {\n  return prisma.$executeRaw(`DELETE FROM sessions WHERE id = ${1}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "raw-query-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn raw_sql_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/reports.ts",
        "declare const prisma: any;\ndeclare const id: string;\nexport async function f() {\n  // raw-sql-ok: id is a validated internal UUID, never request-derived\n  return prisma.$queryRawUnsafe(`SELECT * FROM users WHERE id = ${id}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "raw-query-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- insecure-cookie ---

#[test]
fn cookie_set_without_httponly_anywhere_in_the_function_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\ndeclare const token: string;\nexport function login() {\n  res.cookie(\"session\", token);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "insecure-cookie");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn cookie_set_with_httponly_option_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\ndeclare const token: string;\nexport function login() {\n  res.cookie(\"session\", token, { httpOnly: true, secure: true });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "insecure-cookie").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn cookie_ok_marker_above_the_cookie_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\ndeclare const token: string;\nexport function login() {\n  // cookie-ok: non-sensitive UI preference cookie, not session/auth\n  res.cookie(\"theme\", token);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "insecure-cookie").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- cors-wildcard ---

#[test]
fn wildcard_access_control_allow_origin_header_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/middleware.ts",
        "declare const res: any;\nexport function setCors() {\n  res.setHeader(\"Access-Control-Allow-Origin\", \"*\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-wildcard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn wildcard_origin_config_property_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { origin: '*' };\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "cors-wildcard").len(), 1, "{:?}", out.findings);
}

#[test]
fn allowlisted_origin_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { origin: 'https://example.com' };\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cors-wildcard").is_empty(), "{:?}", out.findings);
}

// --- weak-password-hash ---

#[test]
fn md5_used_on_password_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const password: string;\ndeclare function md5(s: string): string;\nexport const hash = md5(password);\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-password-hash");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn sha1_used_on_password_reversed_order_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const password: string;\ndeclare function hashWith(s: string, algo: string): string;\nexport const h = hashWith(password, \"SHA1\");\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "weak-password-hash").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn bcrypt_with_single_digit_cost_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const bcrypt: any;\ndeclare const password: string;\nexport const hash = bcrypt.hashSync(password, 4);\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "weak-password-hash").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn bcrypt_with_double_digit_cost_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const bcrypt: any;\ndeclare const password: string;\nexport const hash = bcrypt.hashSync(password, 12);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-password-hash").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sha256_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const crypto: any;\ndeclare const password: string;\nexport const hash = crypto.createHash(\"sha256\").update(password).digest(\"hex\");\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-password-hash").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn weak_hash_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const password: string;\ndeclare function md5(s: string): string;\n// weak-hash-ok: legacy checksum for cache-busting, not used for auth\nexport const hash = md5(password);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-password-hash").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- api-key-in-url ---

#[test]
fn api_key_query_param_in_url_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/client.ts",
        "export const url = \"https://api.example.com/data?api_key=abc123\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "api-key-in-url");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn access_token_query_param_in_url_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/client.ts",
        "export const url = \"https://api.example.com/oauth/callback?access_token=xyz789\";\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "api-key-in-url").len(), 1, "{:?}", out.findings);
}

#[test]
fn url_with_no_secret_param_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/client.ts",
        "export const url = \"https://api.example.com/data?id=42\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "api-key-in-url").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn url_key_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/client.ts",
        "// url-key-ok: short-lived one-time token for a third-party webhook callback\nexport const url = \"https://api.example.com/data?api_key=abc123\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "api-key-in-url").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- annotation-sql-concat (Java) ---

#[test]
fn jpa_query_annotation_with_string_concatenation_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/UserRepository.java",
        "public interface UserRepository {\n    @Query(\"SELECT u FROM User u WHERE u.name = '\" + name + \"'\")\n    User findByName(String name);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "annotation-sql-concat");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn jpa_query_annotation_with_named_parameter_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/UserRepository.java",
        "public interface UserRepository {\n    @Query(\"SELECT u FROM User u WHERE u.name = :name\")\n    User findByName(String name);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "annotation-sql-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn query_concat_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/UserRepository.java",
        "public interface UserRepository {\n    // query-concat-ok: name is validated against an internal enum before this call\n    @Query(\"SELECT u FROM User u WHERE u.name = '\" + name + \"'\")\n    User findByName(String name);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "annotation-sql-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- open-redirect ---

#[test]
fn redirect_of_a_request_derived_target_in_the_same_function_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\nexport function handleRedirect(req: any) {\n  const target = req.query.next;\n  res.redirect(target);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "open-redirect");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn redirect_to_a_hardcoded_path_with_no_request_input_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\nexport function goHome() {\n  res.redirect(\"/home\");\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "open-redirect").is_empty(), "{:?}", out.findings);
}

#[test]
fn redirect_ok_marker_above_the_redirect_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\nexport function handleRedirect(req: any) {\n  const target = req.query.next;\n  // redirect-ok: target validated against an internal allow-list above\n  res.redirect(target);\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "open-redirect").is_empty(), "{:?}", out.findings);
}

// --- ssrf-user-url ---

#[test]
fn fetch_of_a_request_derived_url_in_the_same_function_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/proxy.ts",
        "declare const fetch: any;\nexport async function proxy(req: any) {\n  const url = req.query.url;\n  return fetch(url);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "ssrf-user-url");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn fetch_of_a_hardcoded_url_with_no_request_input_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/proxy.ts",
        "declare const fetch: any;\nexport async function ping() {\n  return fetch(\"https://internal.example.com/health\");\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "ssrf-user-url").is_empty(), "{:?}", out.findings);
}

// --- path-traversal ---

#[test]
fn fs_read_of_a_path_joined_request_param_in_the_same_function_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "import * as fs from \"fs\";\nimport * as path from \"path\";\ndeclare const baseDir: string;\nexport async function readUserFile(req: any) {\n  const p = path.join(baseDir, req.params.filename);\n  return fs.readFileSync(p);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "path-traversal");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

#[test]
fn fs_read_of_a_fixed_path_with_no_request_input_or_path_join_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "import * as fs from \"fs\";\nexport function readConfig() {\n  return fs.readFileSync(\"/etc/app/config.json\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "path-traversal").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- cors-credentials-wildcard ---

#[test]
fn credentials_true_alongside_a_wildcard_origin_in_the_same_file_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = {\n  origin: '*',\n  credentials: true,\n};\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-credentials-wildcard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn credentials_true_with_a_specific_origin_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = {\n  origin: 'https://example.com',\n  credentials: true,\n};\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "cors-credentials-wildcard").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- jwt-no-expiry ---

#[test]
fn jwt_sign_with_no_expires_in_anywhere_in_the_function_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issueToken() {\n  return jwt.sign(payload, \"secret\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "jwt-no-expiry");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn jwt_sign_with_expires_in_option_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issueToken() {\n  return jwt.sign(payload, \"secret\", { expiresIn: \"1h\" });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "jwt-no-expiry").is_empty(), "{:?}", out.findings);
}

// --- weak-token-random ---

#[test]
fn math_random_with_token_keyword_before_it_on_the_line_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/token.ts",
        "export function makeToken() {\n  const token = Math.random().toString(36);\n  return token;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-token-random");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn math_random_with_secret_keyword_after_it_on_the_line_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/token.ts",
        "export function makeSecretSuffix() {\n  const value = Math.random().toString() + \"-secret\";\n  return value;\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "weak-token-random").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn math_random_with_no_security_keyword_on_the_line_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/token.ts",
        "export function randomDelay() {\n  const delay = Math.random() * 1000;\n  return delay;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-token-random").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn weak_random_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/token.ts",
        "export function makeToken() {\n  // weak-random-ok: non-security cache-busting value, not used for auth\n  const token = Math.random().toString(36);\n  return token;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-token-random").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- timing-unsafe-compare ---

#[test]
fn strict_equality_compare_of_a_token_shaped_identifier_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const token: string;\ndeclare const expectedToken: string;\nexport function checkToken() {\n  return token === expectedToken;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "timing-unsafe-compare");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn strict_equality_compare_of_a_non_secret_identifier_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/status.ts",
        "declare const status: string;\nexport function isActive() {\n  return status === \"active\";\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "timing-unsafe-compare").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn timing_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const token: string;\ndeclare const expectedToken: string;\nexport function checkToken() {\n  // timing-ok: token is a public request id, not a secret compared for auth\n  return token === expectedToken;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "timing-unsafe-compare").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn presence_check_of_a_token_against_undefined_is_not_flagged() {
    // `tokens !== undefined` is a presence/existence check on a variable whose name happens to
    // contain "token", not a secret-value comparison.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const tokens: string[] | undefined;\nexport function hasTokens() {\n  return tokens !== undefined;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "timing-unsafe-compare").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn null_check_of_a_secret_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const secretValue: string | null;\nexport function hasSecret() {\n  return secretValue === null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "timing-unsafe-compare").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn typeof_guard_of_a_token_shaped_identifier_is_not_flagged() {
    // `typeof tokenData === 'object'` is a type-guard on the VALUE'S TYPE, never a comparison of two
    // secret values, so it structurally cannot be the timing side-channel this rule is about.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const tokenData: unknown;\nexport function normalize() {\n  return typeof tokenData === 'object' ? tokenData : {};\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "timing-unsafe-compare").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn typeof_guard_through_a_property_chain_is_not_flagged() {
    // The checked expression can be a property chain (`data.apiKey`, `req.query.token`), not a bare
    // identifier — the `typeof` exclusion must still fire.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const data: { apiKey?: unknown };\nexport function readKey() {\n  return typeof data.apiKey === 'string' ? data.apiKey : undefined;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "timing-unsafe-compare").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn bare_strict_equality_still_flagged_even_when_typeof_guards_a_different_identifier() {
    // Regression guard: the `typeof` exclusion is keyed to the SAME guard-word identifier it guards —
    // an unrelated `typeof` check elsewhere in the codebase must not blanket-suppress a real secret
    // comparison. (This fixture keeps them on separate lines, which is the common real shape; the
    // exclude_pattern's line-level granularity is a documented, pre-existing trade-off, not new here.)
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const token: string;\ndeclare const expectedToken: string;\nexport function checkToken(x: unknown) {\n  const isStr = typeof x === 'string';\n  return isStr && token === expectedToken;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "timing-unsafe-compare");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

// --- error-leak-to-client ---

#[test]
fn raw_error_sent_via_res_status_5xx_json_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/errors.ts",
        "declare const res: any;\nexport function handleError(err: any) {\n  res.status(500).json(err);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "error-leak-to-client");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn raw_error_sent_via_hono_c_json_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/errors.ts",
        "declare const c: any;\nexport function handleError(err: any) {\n  return c.json(err);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "error-leak-to-client").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn generic_error_message_sent_to_the_client_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/errors.ts",
        "declare const res: any;\nexport function handleError(err: any) {\n  console.error(err);\n  res.status(500).json({ error: \"Internal server error\" });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "error-leak-to-client").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- secret-env-in-fe ---

#[test]
fn server_only_secret_env_var_referenced_in_a_tsx_file_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/components/ApiKeyBanner.tsx",
        "export const key = process.env.SUPABASE_SERVICE_ROLE_KEY;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "secret-env-in-fe");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn public_env_var_referenced_in_a_tsx_file_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/components/PublicConfig.tsx",
        "export const apiUrl = process.env.NEXT_PUBLIC_API_URL;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "secret-env-in-fe").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- localstorage-jwt ---

#[test]
fn token_written_to_local_storage_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "web/auth.ts",
        "export function saveToken(token: string) {\n  localStorage.setItem(\"token\", token);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "localstorage-jwt");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn non_token_value_written_to_local_storage_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "web/prefs.ts",
        "export function saveTheme(theme: string) {\n  localStorage.setItem(\"theme\", theme);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localstorage-jwt").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- java-hardcoded-password (Java) ---

#[test]
fn direct_password_field_assignment_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Config.java",
        "public class Config {\n    public String password = \"sup3rSecretPwd\";\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "java-hardcoded-password");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn jdbc_get_connection_with_literal_credentials_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Db.java",
        "public class Db {\n    public Connection connect() throws Exception {\n        return DriverManager.getConnection(\"jdbc:mysql://host/db\", \"admin\", \"p@ssw0rd\");\n    }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "java-hardcoded-password").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn password_read_from_env_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Config.java",
        "public class Config {\n    private String password = System.getenv(\"DB_PASSWORD\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "java-hardcoded-password").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn java_pwd_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Config.java",
        "public class Config {\n    // java-pwd-ok: test fixture placeholder, rotated dummy value\n    public String password = \"sup3rSecretPwd\";\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "java-hardcoded-password").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- xxe-no-guard (Java) ---

#[test]
fn document_builder_factory_with_no_guard_in_the_method_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "xxe-no-guard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn document_builder_factory_with_disallow_doctype_decl_guard_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        factory.setFeature(\"http://apache.org/xml/features/disallow-doctype-decl\", true);\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "xxe-no-guard").is_empty(), "{:?}", out.findings);
}

#[test]
fn feature_secure_processing_alone_no_longer_suffices_and_is_now_flagged() {
    // Per OWASP, FEATURE_SECURE_PROCESSING alone does NOT disable external entity resolution — the
    // matcher's `absent` veto list used to treat it as a sufficient guard on its own (a single combined
    // "disallow-doctype-decl|FEATURE_SECURE_PROCESSING" entry); now only disallow-doctype-decl=true or
    // both external-entities-false vetoes, so FSP-alone must fire.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        factory.setFeature(XMLConstants.FEATURE_SECURE_PROCESSING, true);\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "xxe-no-guard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn document_builder_factory_with_both_external_entities_disabled_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        factory.setFeature(\"http://xml.org/sax/features/external-general-entities\", false);\n        factory.setFeature(\"http://xml.org/sax/features/external-parameter-entities\", false);\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "xxe-no-guard").is_empty(), "{:?}", out.findings);
}

#[test]
fn document_builder_factory_with_only_external_general_entities_disabled_is_not_flagged() {
    // Documents the matcher's actual (intentionally disclosed in the message) OR semantics: each
    // `absent` entry vetoes independently, so a single recognized guard line is enough even though the
    // message recommends setting BOTH external-general-entities and external-parameter-entities.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        factory.setFeature(\"http://xml.org/sax/features/external-general-entities\", false);\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "xxe-no-guard").is_empty(), "{:?}", out.findings);
}

#[test]
fn xxe_ok_marker_in_the_method_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        // xxe-ok: guard applied via a shared factory helper not visible in this method\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "xxe-no-guard").is_empty(), "{:?}", out.findings);
}

// --- unsafe-deserialization (Java) ---

#[test]
fn object_input_stream_read_object_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Loader.java",
        "public class Loader {\n    public Object load(byte[] data) throws Exception {\n        ObjectInputStream ois = new ObjectInputStream(new ByteArrayInputStream(data));\n        return ois.readObject();\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "unsafe-deserialization");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn json_deserialization_with_no_object_input_stream_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Loader.java",
        "public class Loader {\n    public Object load(String json) {\n        return objectMapper.readValue(json, Object.class);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unsafe-deserialization").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- java-path-traversal (Java) ---

#[test]
fn new_file_built_from_a_request_parameter_in_the_same_method_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/FileController.java",
        "public class FileController {\n    public void download(HttpServletRequest request) throws IOException {\n        String filename = request.getParameter(\"file\");\n        File file = new File(\"/uploads/\" + filename);\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "java-path-traversal");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn new_file_with_a_fixed_path_and_no_request_parameter_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/FileController.java",
        "public class FileController {\n    public void download() throws IOException {\n        File file = new File(\"/uploads/report.pdf\");\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "java-path-traversal").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- java-weak-random (Java) ---

#[test]
fn new_random_with_token_keyword_before_it_on_the_line_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/TokenGenerator.java",
        "public class TokenGenerator {\n    public String makeToken() {\n        String token = String.valueOf(new Random().nextLong());\n        return token;\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "java-weak-random");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn new_random_with_session_keyword_after_it_on_the_line_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/SessionUtil.java",
        "public class SessionUtil {\n    public String makeSessionId() {\n        return new Random().nextLong() + \"-session\";\n    }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "java-weak-random").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn new_random_with_no_security_keyword_on_the_line_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/DiceRoller.java",
        "public class DiceRoller {\n    public int roll() {\n        return new Random().nextInt(6) + 1;\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "java-weak-random").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- stacktrace-to-response (Java) ---

#[test]
fn print_stack_trace_in_a_method_that_also_returns_a_response_entity_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/ApiController.java",
        "public class ApiController {\n    public ResponseEntity<String> handle(Exception e) {\n        e.printStackTrace();\n        return ResponseEntity.status(500).body(e.getMessage());\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "stacktrace-to-response");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn print_stack_trace_with_no_response_object_in_the_method_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Worker.java",
        "public class Worker {\n    public void process(Exception e) {\n        e.printStackTrace();\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "stacktrace-to-response").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- trust-all-tls (Java) ---

#[test]
fn trust_all_certs_class_instantiation_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/InsecureSslContext.java",
        "public class InsecureSslContext {\n    public X509TrustManager trustAllCerts = new TrustAllCerts();\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "trust-all-tls");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn allow_all_hostname_verifier_constant_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/HttpClientConfig.java",
        "public class HttpClientConfig {\n    public void configure(HttpClient client) {\n        client.setHostnameVerifier(SSLConnectionSocketFactory.ALLOW_ALL_HOSTNAME_VERIFIER);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "trust-all-tls").len(), 1, "{:?}", out.findings);
}

#[test]
fn hostname_verifier_lambda_always_returning_true_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/HttpClientConfig.java",
        "public class HttpClientConfig {\n    public void configure(HttpsURLConnection conn) {\n        conn.setHostnameVerifier((hostname, session) -> true);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "trust-all-tls").len(), 1, "{:?}", out.findings);
}

#[test]
fn hostname_verifier_using_the_default_implementation_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/HttpClientConfig.java",
        "public class HttpClientConfig {\n    public void configure(HttpsURLConnection conn) {\n        conn.setHostnameVerifier(HttpsURLConnection.getDefaultHostnameVerifier());\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "trust-all-tls").is_empty(), "{:?}", out.findings);
}

#[test]
fn trust_all_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/InsecureSslContext.java",
        "public class InsecureSslContext {\n    // trust-all-ok: used only in a local dev test harness against a self-signed cert\n    public X509TrustManager trustAllCerts = new TrustAllCerts();\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "trust-all-tls").is_empty(), "{:?}", out.findings);
}

// --- conn-string-credentials ---

#[test]
fn postgres_connection_string_with_password_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = \"postgres://user:hunter2@host:5432/db\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "conn-string-credentials");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn redis_connection_string_with_password_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cache.ts",
        "export const url = \"redis://user:pass123@host:6379\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "conn-string-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn mongodb_srv_connection_string_with_password_is_flagged() {
    // Also exercises the general RFC 3986 scheme grammar (`[a-z][a-z0-9+.-]*`) against a scheme
    // carrying a `+` (`mongodb+srv`), not just a plain alphabetic scheme.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/mongo.ts",
        "export const url = \"mongodb+srv://admin:realsecret@cluster.mongodb.net/db\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "conn-string-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn connection_string_with_env_var_interpolation_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = `postgres://user:${process.env.DB_PASSWORD}@host:5432/db`;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn connection_string_with_angle_bracket_placeholder_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = \"postgres://user:<password>@host:5432/db\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn connection_string_with_mustache_placeholder_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = \"postgres://user:{{password}}@host:5432/db\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn real_credential_with_a_ts_generic_elsewhere_on_the_line_is_still_flagged() {
    // The placeholder vetoes are anchored to the URL's userinfo (between `://` and `@`) — a TS
    // generic's angle brackets outside the URL must not suppress a real literal credential.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const c: Record<string, string> = { db: \"postgres://svc:S3cr3tPw9@host:5432/db\" };\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "conn-string-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn real_credential_with_interpolated_host_after_the_at_sign_is_still_flagged() {
    // `${...}` interpolation in the HOST slot does not launder a literal password in the
    // userinfo slot — only a placeholder between `://` and `@` vetoes.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = `postgres://admin:S3cr3tPw9@${host}/db`;\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "conn-string-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn loopback_host_credential_is_not_flagged() {
    // immich dogfood (round 7): `postgres://postgres:postgres@localhost:5432/immich` in a dev
    // script — a credential that only answers on loopback is not a remotely usable leak.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/bin/sync.ts",
        "process.env.DB_URL = 'postgres://postgres:postgres@localhost:5432/immich';\nconst alt = \"redis://user:realish@127.0.0.1:6379\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn loopback_lookalike_host_still_fires() {
    // `localhost.evil.com` is NOT loopback — the veto must not match a prefix of a real host.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/db.ts",
        "export const url = \"postgres://svc:S3cr3tPw9@localhost.evil.com:5432/db\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "conn-string-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn placeholder_substring_password_is_not_flagged() {
    // immich dogfood (round 7): spec fixtures use `mypg:mypwd@myhost` — the placeholder word
    // (`pwd`) sits inside a longer token, so the veto matches placeholder words as substrings.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/services/backup.spec.ts",
        "const dbUrl = 'postgresql://mypg:mypwd@myhost:1234/myimmich?sslmode=require';\nconst two = \"amqp://svc:examplePass2@mq.internal:5672\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn product_default_password_is_not_flagged() {
    // immich dogfood (round 7): `postgres://postgres1:postgres2@database1:54320/immich` spec
    // fixture — a database product name (optionally digit-suffixed) in the password slot is a
    // default/metasyntactic credential, not a leaked secret. Deliberate tradeoff: a REAL
    // unchanged-default (`:root@`) is a weak-default problem, not a committed-secret leak —
    // out of this rule's scope.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/repositories/config.repository.spec.ts",
        "process.env.DB_URL = 'postgres://postgres1:postgres2@database1:54320/immich';\nconst r = \"redis://svc:redis@cache.internal:6379\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn connection_string_with_changeme_placeholder_word_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = \"postgres://user:changeme@host:5432/db\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn connection_string_with_password_placeholder_word_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = \"postgres://user:password@host:5432/db\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn conn_cred_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "// conn-cred-ok: local docker-compose sample connection string, not a real credential\nexport const url = \"postgres://user:hunter2@host:5432/db\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- skip_comment_lines + test-path file_exclude_pattern ---
// Without `skip_comment_lines`, a commented-out example of a matched shape (e.g. the `mass-assignment`
// body-passthrough shape) would fire on `method-scan` rules. Deployed-surface rules in this pack
// (everything except `hardcoded-secret`/`java-hardcoded-password`) exclude test-path files via the
// shared `file_exclude_pattern`.

#[test]
fn mass_assignment_shape_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/users.ts",
        "declare const prisma: any;\nexport async function updateUser(req: any) {\n  // prisma.user.update({ data: req.body }) -- old unsafe version, replaced below\n  return null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mass-assignment").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn cookie_set_without_httponly_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/__tests__/auth.test.ts",
        "declare const res: any;\ndeclare const token: string;\nexport function login() {\n  res.cookie(\"session\", token);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "insecure-cookie").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn hardcoded_secret_in_a_test_fixture_path_is_still_flagged() {
    // `hardcoded-secret` (and `java-hardcoded-password`) are repo-content rules, not deployed-surface,
    // so unlike the rest of this pack they don't exclude test-fixture paths — a real secret committed
    // inside a test file is still a leaked credential the moment it's pushed.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/__tests__/config.test.ts",
        "export const apiKey = \"abcd1234efgh5678\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

// --- private-key-committed ---

#[test]
fn rsa_private_key_pem_header_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "config/keys.ts",
        "export const key = `-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEAx1n...\n-----END RSA PRIVATE KEY-----`;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "private-key-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn unlabeled_pkcs8_private_key_pem_header_is_flagged() {
    // The `(RSA |EC |DSA |OPENSSH |ENCRYPTED |PGP )?` prefix group is optional — a bare PKCS8
    // "PRIVATE KEY" header (no algorithm prefix) must still fire.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "secrets/id_rsa.pem",
        "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQ...\n-----END PRIVATE KEY-----\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "private-key-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn openssh_private_key_header_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "deploy/config.yaml",
        "sshKey: |\n  -----BEGIN OPENSSH PRIVATE KEY-----\n  b3BlbnNzaC1rZXktdjEAAAAA...\n  -----END OPENSSH PRIVATE KEY-----\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "private-key-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn private_key_committed_in_a_test_fixture_path_is_still_flagged() {
    // Repo-content rule: an actual PEM header in a test fixture is still a committed key.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/__tests__/fixtures.test.ts",
        "export const testKey = `-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEAx1n...\n-----END RSA PRIVATE KEY-----`;\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "private-key-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn private_key_header_generated_via_template_interpolation_is_not_flagged() {
    // exclude_pattern claim: an interpolation shape (`${`/`{{`) ON THE HEADER LINE itself reads as a
    // key-template generator, not a literal committed key.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "scripts/keygen-template.ts",
        "export const header = `-----BEGIN ${keyType} PRIVATE KEY-----`;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "private-key-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn private_key_ok_marker_above_the_header_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "config/keys.ts",
        "// private-key-ok: throwaway key generated only for this test, never used against a real service\nexport const key = `-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEAx1n...\n-----END RSA PRIVATE KEY-----`;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "private-key-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn non_pem_looking_dashes_are_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "docs/notes.ts",
        "export const divider = \"----- a plain divider, not a key -----\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "private-key-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pem_header_mentioned_in_prose_without_a_key_body_is_not_flagged() {
    // Class fix (mono-hub 0.10.0 FP): an i18n/doc string that merely NAMES the PEM header — no
    // base64 key material accompanies it — is a non-key reading, so the header substring alone
    // must not fire. The line_pattern requires either the header at end-of-line (a dedicated key
    // line) or a base64 body after it; a header wrapped in a prose sentence satisfies neither.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/i18n/en.json",
        "{\n  \"guide\": \"Running keygen produces a `-----BEGIN PRIVATE KEY-----` block within a few seconds.\"\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "private-key-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn single_line_json_embedded_private_key_with_base64_body_is_flagged() {
    // The complement of the prose fixture: a real key committed as a single JSON string (header,
    // an escaped `\n`, then the base64 body all on one physical line) must still fire — the
    // base64-body alternative of the line_pattern is what catches it.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "config/secrets.json",
        "{ \"privateKey\": \"-----BEGIN PRIVATE KEY-----\\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSj\\n-----END PRIVATE KEY-----\" }\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "private-key-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

// --- vendor-token-committed ---
//
// These fixtures must carry live vendor-token SHAPES by design (that is what the rule detects),
// but GitHub push-protection scans raw source text for those same shapes. So each Stripe token is
// assembled from split literals via `concat!` — no contiguous `sk_live_`/`rk_live_`/`sk_test_`
// string ever appears in this file, while the rule still analyzes the full reassembled token in the
// synthetic file content. Bodies are obviously-synthetic (not real keys).
const STRIPE_LIVE: &str = concat!("sk_li", "ve_FAKEexampleonly0notarealkey01");
const STRIPE_RK: &str = concat!("rk_li", "ve_FAKEexampleonly0notarealkey01");
const STRIPE_TEST: &str = concat!("sk_te", "st_FAKEexampleonly0notarealkey01");
const GH_PAT: &str = concat!("gh", "p_abcdefghij1234567890ABCDEFGHIJ123456");
const GH_OAUTH: &str = concat!("gh", "o_abcdefghij1234567890ABCDEFGHIJ123456");
const SLACK_BOT: &str = concat!("xo", "xb-1234567890-abcdEFGH1234");
const GOOGLE_API: &str = concat!("AI", "zaabcdefghij1234567890ABCDEFGHIJ12345");

#[test]
fn stripe_live_secret_key_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/billing.ts",
        &format!("export const stripeKey = \"{STRIPE_LIVE}\";\n"),
    );
    let out = scan(&dir);
    let h = hits(&out, "vendor-token-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn stripe_live_restricted_key_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/billing.ts",
        &format!("export const stripeRestrictedKey = \"{STRIPE_RK}\";\n"),
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "vendor-token-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn stripe_test_secret_key_is_deliberately_not_flagged() {
    // Documented claim: sk_test_ is a test-mode key, not a production credential, and is
    // deliberately NOT matched.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/billing.ts",
        &format!("export const stripeKey = \"{STRIPE_TEST}\";\n"),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "vendor-token-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn github_personal_access_token_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "scripts/deploy.ts",
        &format!("export const ghToken = \"{GH_PAT}\";\n"),
    );
    let out = scan(&dir);
    let h = hits(&out, "vendor-token-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn github_oauth_token_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "scripts/deploy.ts",
        &format!("export const ghToken = \"{GH_OAUTH}\";\n"),
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "vendor-token-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn slack_bot_token_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "integrations/slack.ts",
        &format!("export const slackToken = \"{SLACK_BOT}\";\n"),
    );
    let out = scan(&dir);
    let h = hits(&out, "vendor-token-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn google_api_key_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "config/maps.ts",
        &format!("export const mapsKey = \"{GOOGLE_API}\";\n"),
    );
    let out = scan(&dir);
    let h = hits(&out, "vendor-token-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn stripe_live_key_read_from_process_env_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/billing.ts",
        &format!("export const stripeKey = process.env.STRIPE_KEY || \"{STRIPE_LIVE}\";\n"),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "vendor-token-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn vendor_token_committed_in_a_test_fixture_path_is_still_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/__tests__/billing.test.ts",
        &format!("export const stripeKey = \"{STRIPE_LIVE}\";\n"),
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "vendor-token-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn vendor_token_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/billing.ts",
        &format!("// vendor-token-ok: rotated dummy value kept only for a format-parsing regression test\nexport const stripeKey = \"{STRIPE_LIVE}\";\n"),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "vendor-token-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- jwt-none-algorithm ---

#[test]
fn algorithms_array_containing_none_in_a_jwt_file_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import jwt from \"jsonwebtoken\";\nexport function verify(token: string) {\n  return jwt.verify(token, \"\", { algorithms: [\"none\"] });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "jwt-none-algorithm");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn bare_algorithm_none_in_a_jwt_file_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import jwt from \"jsonwebtoken\";\nexport const opts = { algorithm: 'none' };\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "jwt-none-algorithm").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn algorithms_hs256_in_a_jwt_file_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import jwt from \"jsonwebtoken\";\nexport function verify(token: string) {\n  return jwt.verify(token, \"secret\", { algorithms: [\"HS256\"] });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-none-algorithm").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn algorithm_none_with_no_jwt_library_gate_present_is_not_flagged() {
    // require_file gate claim: without a jwt/jose/jsonwebtoken token anywhere in the file, an
    // unrelated `algorithm: 'none'`-shaped config is not opted into this rule.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "config/compression.ts",
        "export const opts = { algorithm: 'none' };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-none-algorithm").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn jwt_none_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import jwt from \"jsonwebtoken\";\nexport function verify(token: string) {\n  // jwt-none-ok: local attack-simulation test harness, never runs against a real service\n  return jwt.verify(token, \"\", { algorithms: [\"none\"] });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-none-algorithm").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- shell-exec-interpolation ---

#[test]
fn exec_with_template_literal_interpolation_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { exec } from \"child_process\";\nexport function run(name: string) {\n  exec(`ls ${name}`);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "shell-exec-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn exec_sync_with_string_concatenation_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.js",
        "const { execSync } = require(\"child_process\");\nfunction run(name) {\n  execSync(\"ls \" + name);\n}\nmodule.exports = { run };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "shell-exec-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn cp_member_exec_with_template_interpolation_is_flagged() {
    // Member form: only the known child_process receiver aliases (`child_process`/`childProcess`/
    // `cp`) fire — the allowlist is what keeps RegExp's `.exec(` out (see the regexp fixture below).
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import * as cp from \"child_process\";\nexport function run(name: string) {\n  cp.exec(`ls ${name}`);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "shell-exec-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn regexp_exec_with_dynamic_arg_is_not_flagged() {
    // Reviewer-verified FP shape: `pattern.exec(...)` is RegExp.prototype.exec, not a shell — a
    // plain `\b(?:exec|execSync)` boundary is satisfied at the `.`->`e` transition, so the matcher
    // instead requires a non-dot/word char before a bare `exec` (dot-guard idiom) and allows member
    // calls only on the known child_process receiver aliases. The file mentions child_process on
    // purpose, so this pins the dot-guard itself, not just the require_file gate.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/version.ts",
        "import { execFile } from \"child_process\";\ndeclare const pattern: RegExp;\ndeclare const version: string;\ndeclare const x: string;\nexport const m1 = pattern.exec(`v${version}`);\nexport const m2 = pattern.exec(\"pre\" + x);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn bare_exec_interpolation_without_a_child_process_mention_is_gate_skipped() {
    // require_file gate claim: a file that never mentions `child_process` cannot be shelling out
    // through it, so a same-named local `exec` helper with an interpolated arg stays silent.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/query.ts",
        "declare function exec(q: string): unknown;\nexport function run(table: string) {\n  return exec(`analyze ${table}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn exec_with_a_fixed_string_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { exec } from \"child_process\";\nexport function cleanup() {\n  exec(\"rm -rf /tmp/cache\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn exec_file_with_argv_array_and_interpolated_arg_is_not_flagged() {
    // Documented boundary: execFile/spawn (argv-array APIs) are deliberately not matched, even
    // when one of their array elements is itself interpolated.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { execFile } from \"child_process\";\nexport function run(name: string) {\n  execFile(\"ls\", [`${name}`]);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn spawn_with_argv_array_and_interpolated_arg_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { spawn } from \"child_process\";\nexport function run(name: string) {\n  spawn(\"ls\", [`${name}`]);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn shell_exec_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { exec } from \"child_process\";\nexport function run(name: string) {\n  // shell-exec-ok: name is validated against an internal allow-list above\n  exec(`ls ${name}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- jwt-sign-literal-secret ---

#[test]
fn jwt_sign_with_a_positional_literal_secret_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issue() {\n  return jwt.sign(payload, \"abcd1234efgh5678\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "jwt-sign-literal-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn jwt_sign_with_a_multi_key_payload_and_literal_secret_is_flagged() {
    // Reviewer-verified miss shape under the old `[^,]*` pattern: a multi-key payload object puts
    // commas BEFORE the secret argument, so binding to the FIRST comma missed the literal entirely.
    // The greedy `.*` now reaches the last comma before a closing-position literal.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const u: any;\nexport function issue() {\n  return jwt.sign({ userId: u.id, role: u.role }, \"hardcoded1234\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "jwt-sign-literal-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn jwt_sign_with_a_quoted_property_key_and_variable_secret_is_not_flagged() {
    // The `\s*[,)]` tail binds the literal to an ARGUMENT position — a quoted alnum property key
    // inside the payload object is followed by `:`, so it can't satisfy the tail and must not fire.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const v: any;\ndeclare const opts: any;\nexport function issue() {\n  return jwt.sign({ \"role12345\": v }, opts);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-sign-literal-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn jwt_sign_with_secret_read_from_process_env_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issue() {\n  return jwt.sign(payload, process.env.JWT_SECRET);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-sign-literal-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn jwt_sign_with_a_variable_secret_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\ndeclare const secretKey: string;\nexport function issue() {\n  return jwt.sign(payload, secretKey);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-sign-literal-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn jwt_sign_with_a_mock_prefixed_literal_secret_is_not_flagged() {
    // Reuses hardcoded-secret's placeholder-word veto family.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issue() {\n  return jwt.sign(payload, \"mock-abcd1234\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-sign-literal-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn jwt_sign_boundary_positional_literal_is_uncovered_by_hardcoded_secret() {
    // Boundary claim: hardcoded-secret's assignment pattern needs a `key: value`/`key = value`
    // shape, so this positional `jwt.sign(payload, "literal")` form does not trip it — only
    // jwt-sign-literal-secret catches it.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issue() {\n  return jwt.sign(payload, \"abcd1234efgh5678\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
    assert_eq!(
        hits(&out, "jwt-sign-literal-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn jwt_sign_literal_secret_in_a_test_fixture_path_is_still_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/__tests__/auth.test.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issue() {\n  return jwt.sign(payload, \"abcd1234efgh5678\");\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "jwt-sign-literal-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn jwt_secret_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issue() {\n  // jwt-secret-ok: rotated test-only fixture key, not a real credential\n  return jwt.sign(payload, \"abcd1234efgh5678\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-sign-literal-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- jwt-verify-bypass ---

#[test]
fn ignore_expiration_true_in_a_jsonwebtoken_file_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import jwt from \"jsonwebtoken\";\nexport function verify(token: string) {\n  return jwt.verify(token, \"secret\", { ignoreExpiration: true });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "jwt-verify-bypass");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn verify_false_in_a_jsonwebtoken_file_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import jwt from \"jsonwebtoken\";\nexport const opts = { verify: false };\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "jwt-verify-bypass").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn verify_true_in_a_jsonwebtoken_file_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import jwt from \"jsonwebtoken\";\nexport function verify(token: string) {\n  return jwt.verify(token, \"secret\", { ignoreExpiration: false });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-verify-bypass").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn verify_false_with_no_jwt_library_gate_present_is_not_flagged() {
    // require_file gate claim: bare `verify: false` shows up in unrelated (e.g. bundler-ish)
    // configs too — pinned negative in a webpack-shaped file with no jsonwebtoken/jose/jwt token
    // anywhere in it.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "webpack.extra.config.ts",
        "export const moduleRules = { verify: false, cache: true };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-verify-bypass").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn jwt_verify_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import jwt from \"jsonwebtoken\";\nexport function verify(token: string) {\n  // jwt-verify-ok: dedicated expired-token regression test, not production code\n  return jwt.verify(token, \"secret\", { ignoreExpiration: true });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-verify-bypass").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- sendfile-from-request ---

#[test]
fn send_file_of_a_request_derived_path_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "declare const res: any;\nexport function download(req: any) {\n  res.sendFile(req.params.filename);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sendfile-from-request");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn download_of_a_request_derived_path_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "declare const res: any;\nexport function getFile(req: any) {\n  res.download(req.query.path);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "sendfile-from-request").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn send_file_of_a_fixed_path_with_no_request_input_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "declare const res: any;\nexport function downloadReport() {\n  res.sendFile(\"/reports/summary.pdf\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sendfile-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn send_file_wrapped_in_path_basename_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "declare const res: any;\ndeclare const path: any;\nexport function download(req: any) {\n  res.sendFile(path.basename(req.params.filename));\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sendfile-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sendfile_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "declare const res: any;\nexport function download(req: any) {\n  // sendfile-ok: filename validated against an internal allow-list above\n  res.sendFile(req.params.filename);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sendfile-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- cors-reflected-origin-credentials ---

#[test]
fn credentials_true_then_origin_true_on_one_line_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { credentials: true, origin: true };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-reflected-origin-credentials");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn origin_true_then_credentials_true_on_one_line_is_flagged() {
    // Same co-occurrence, reversed key order — both orders must fire.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { origin: true, credentials: true };\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "cors-reflected-origin-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn origin_reflecting_request_headers_with_credentials_true_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "declare const req: any;\nexport const corsOptions = { origin: req.headers.origin, credentials: true };\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "cors-reflected-origin-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn credentials_true_with_a_specific_allowlisted_origin_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { credentials: true, origin: 'https://example.com' };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "cors-reflected-origin-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn multiline_cors_options_object_is_a_documented_limitation_and_not_flagged() {
    // Documented, deliberate limitation (not desired behavior): the matcher is single-line
    // co-occurrence, so splitting `origin`/`credentials` across separate lines evades it even
    // though the resulting configuration is exactly as vulnerable as the single-line shape above.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = {\n  origin: true,\n  credentials: true,\n};\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "cors-reflected-origin-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn cors_reflect_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "// cors-reflect-ok: internal-only service mesh endpoint, never exposed publicly\nexport const corsOptions = { credentials: true, origin: true };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "cors-reflected-origin-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- template-unescaped-output ---

fn label_of(f: &zzop_core::Finding) -> Option<&str> {
    f.data
        .as_ref()
        .and_then(|d| d.get("label"))
        .and_then(|v| v.as_str())
}

#[test]
fn ejs_raw_output_tag_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("views/item.ejs", "<div>\n<%- widgetHtml %>\n</div>\n");
    let out = scan(&dir);
    let h = hits(&out, "template-unescaped-output");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
    assert_eq!(label_of(h[0]), Some("ejs-raw"));
}

#[test]
fn ejs_escaped_output_tag_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("views/item.ejs", "<div>\n<%= user.name %>\n</div>\n");
    let out = scan(&dir);
    assert!(
        hits(&out, "template-unescaped-output").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handlebars_triple_stache_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("views/item.hbs", "<div>\n{{{ rawHtml }}}\n</div>\n");
    let out = scan(&dir);
    let h = hits(&out, "template-unescaped-output");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
    assert_eq!(label_of(h[0]), Some("handlebars-triple"));
}

#[test]
fn handlebars_double_stache_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("views/item.hbs", "<div>\n{{ name }}\n</div>\n");
    let out = scan(&dir);
    assert!(
        hits(&out, "template-unescaped-output").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mustache_amp_unescaped_form_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("views/item.mustache", "<div>\n{{& rawHtml}}\n</div>\n");
    let out = scan(&dir);
    let h = hits(&out, "template-unescaped-output");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("mustache-amp"));
}

#[test]
fn loose_inequality_operator_in_a_template_file_is_not_flagged() {
    // The Pug buffered-unescaped `!= expr` label was deliberately DROPPED (never-guess): it is
    // lexically indistinguishable from the loose-inequality operator `!=`, which appears in Pug and
    // Nunjucks conditionals (`{% if role != "guest" %}`). This pins that a covered template file
    // containing a bare `!=` operator produces NO finding — the whole point of dropping the label.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "views/page.njk",
        "{% if role != \"guest\" %}\n  <p>admin</p>\n{% endif %}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "template-unescaped-output").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn unescaped_template_syntax_in_a_ts_file_is_not_flagged() {
    // Rule is deliberately extension-scoped to template files only: a token like `{{{` is a
    // legitimate JS/TS syntax fragment, so a non-template file must stay silent no matter what it
    // contains.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/util.ts",
        "export function f() {\n  return {{{ a: 1 }};\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "template-unescaped-output").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn template_unescaped_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "views/item.ejs",
        "// template-unescaped-ok: widgetHtml is sanitized upstream via DOMPurify\n<%- widgetHtml %>\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "template-unescaped-output").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- html-response-from-request ---

#[test]
fn res_send_with_req_query_and_html_tag_literal_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/greet.ts",
        "declare const res: any;\ndeclare const req: any;\nexport function greet() {\n  const name = req.query.name;\n  res.send('<div>' + name + '</div>');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "html-response-from-request");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

#[test]
fn res_write_with_req_body_and_content_type_html_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/report.ts",
        "declare const res: any;\ndeclare const req: any;\nexport function report() {\n  res.setHeader('Content-Type', 'text/html');\n  const title = req.body.title;\n  res.write(title);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "html-response-from-request");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

#[test]
fn res_send_with_json_body_and_no_html_marker_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/data.ts",
        "declare const res: any;\ndeclare const req: any;\nexport function data() {\n  const id = req.params.id;\n  res.send({ id });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "html-response-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn res_send_with_html_tag_but_no_request_input_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/static.ts",
        "declare const res: any;\nexport function landing() {\n  res.send('<div>welcome</div>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "html-response-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn res_send_with_sanitizer_present_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/greet.ts",
        "declare const res: any;\ndeclare const req: any;\ndeclare function escapeHtml(s: string): string;\nexport function greet() {\n  const name = req.query.name;\n  res.send('<div>' + escapeHtml(name) + '</div>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "html-response-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn html_response_from_request_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "tests/greet.ts",
        "declare const res: any;\ndeclare const req: any;\nexport function greet() {\n  const name = req.query.name;\n  res.send('<div>' + name + '</div>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "html-response-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn html_response_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/greet.ts",
        "declare const res: any;\ndeclare const req: any;\nexport function greet() {\n  const name = req.query.name;\n  // html-response-ok: name is allow-listed to alpha chars upstream\n  res.send('<div>' + name + '</div>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "html-response-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- dangerous-html-concat ---

#[test]
fn opening_tag_literal_concatenated_with_a_variable_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/render.ts",
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  res.send('<div>' + name);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "dangerous-html-concat");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
    assert_eq!(label_of(h[0]), Some("open-tag-concat"));
}

#[test]
fn variable_concatenated_with_a_closing_tag_literal_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/render.ts",
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  res.send(name + '</div>');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "dangerous-html-concat");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("close-tag-concat"));
}

#[test]
fn pure_literal_html_concatenation_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/render.ts",
        "declare const res: any;\nexport function render() {\n  res.send('<div>' + '</div>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn non_html_concatenation_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/query.ts",
        "declare const res: any;\ndeclare const col: string;\nexport function build() {\n  res.send('SELECT ' + col);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn variable_concatenated_with_a_non_tag_string_ending_in_gt_is_not_flagged() {
    // close-tag-concat tightening: the trailing literal must contain actual tag markup (a `<`
    // before its `>`), so a benign arrow/annotation string that merely ENDS in `>` (`'arrow ->'`)
    // is not mistaken for a closing HTML tag.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/log.ts",
        "declare const res: any;\ndeclare const msg: string;\nexport function render() {\n  res.send(msg + 'arrow -> text');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn html_tag_concatenation_with_no_response_context_gate_present_is_not_flagged() {
    // require_file gate claim: without any res./response./content-type mention anywhere in the
    // file, the concatenation shape alone stays silent (e.g. a CLI/log-formatting helper).
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "cli/format.ts",
        "declare const name: string;\nexport function format() {\n  return '<div>' + name;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn html_concat_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "tests/render.ts",
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  res.send('<div>' + name);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn html_concat_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/render.ts",
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  // html-concat-ok: name is escaped via a wrapper the regex can't see\n  res.send('<div>' + name);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- csp-disabled ---

#[test]
fn helmet_content_security_policy_false_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  contentSecurityPolicy: false,\n}));\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "csp-disabled");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
    assert_eq!(label_of(h[0]), Some("helmet-csp-false"));
}

#[test]
fn csp_header_with_unsafe_inline_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const res: any;\nres.setHeader('Content-Security-Policy', \"default-src 'self' 'unsafe-inline'\");\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "csp-disabled");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("csp-unsafe-inline"));
}

#[test]
fn csp_default_src_wildcard_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const res: any;\nres.setHeader('Content-Security-Policy', \"default-src *\");\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "csp-disabled");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("csp-wildcard"));
}

#[test]
fn helmet_content_security_policy_enabled_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  contentSecurityPolicy: true,\n}));\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "csp-disabled").is_empty(), "{:?}", out.findings);
}

#[test]
fn csp_wildcard_with_no_helmet_gate_present_is_not_flagged() {
    // require_file gate claim: the `csp-wildcard` label's own trigger text (`default-src *`)
    // does not itself contain "helmet"/"content-security-policy"/"contentSecurityPolicy", so it's
    // the one label where the gate is a real, non-tautological constraint — a file that never
    // mentions helmet or CSP anywhere stays silent even though the directive shape matches.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/flags.ts",
        "export const unrelatedConfig = {\n  defaultSrcNote: 'default-src * everywhere',\n};\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "csp-disabled").is_empty(), "{:?}", out.findings);
}

#[test]
fn csp_disabled_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "tests/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  contentSecurityPolicy: false,\n}));\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "csp-disabled").is_empty(), "{:?}", out.findings);
}

#[test]
fn csp_disabled_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  // csp-disabled-ok: CSP is enforced at the CDN edge instead\n  contentSecurityPolicy: false,\n}));\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "csp-disabled").is_empty(), "{:?}", out.findings);
}
