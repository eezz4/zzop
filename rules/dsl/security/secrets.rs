use crate::{hits, scan, TempDir};

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
        // Split literal, same convention (and same reason) as `vendor_token_committed.rs`'s header:
        // GitHub push protection scans RAW SOURCE for a well-formed AWS key id and does not read the
        // comment saying the body is synthetic. `concat!` rejoins it at compile time, so the file
        // content this test writes -- and therefore what the rule sees -- is unchanged.
        concat!("export const key = \"AK", "IAABCDEFGHIJKLMNOP\";\n"),
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
        "// zzop-hardcoded-secret-ok: rotated test-only fixture key, not a real credential\nexport const apiKey = \"abcd1234efgh5678\";\n",
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
    // client-secret shape (not a real credential), split across two literals for the same reason as
    // the AWS-key test above -- `GOCSPX-` is one of the prefixes GitHub push protection blocks.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/ServiceAuthen.java",
        concat!(
            "public class ServiceAuthen {\n    final String AUTHEN_GOOGLE_CLIENT_SECRET = \"GOC",
            "SPX-Ab1Cd2Ef3Gh4Ij5Kl6Mn7Qr8\";\n}\n"
        ),
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

// --- high-entropy-secret (literal-scan over the string-literals channel) ---

/// The RED-FIRST witness for A17: this exact fixture was run against the pre-A17 rule set and produced
/// ZERO findings from ANY rule (captured 2026-08-03, `cargo test -p zzop-rules
/// passphrase_bound_to_secret_name` before the rule landed: `hits(high-entropy-secret)` len 0 AND
/// `out.findings` empty) — the measured passphrase blindness `hardcoded-secret`'s message admits: veto
/// #4 (`["'][A-Za-z]+(?:[-_][A-Za-z]+)+["']`) kills `"correct-horse-battery-staple"` (97.9 total
/// Shannon bits) exactly like an identifier. The literal-scan rule reads entropy the line scan cannot
/// compute, so this now fires.
#[test]
fn passphrase_bound_to_secret_name_is_flagged_and_line_scan_still_silent() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const dbPassword = 'correct-horse-battery-staple';\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "high-entropy-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
    // The line-scan twin stays silent on this shape — its veto #4 is unchanged; the passphrase class
    // is the NEW rule's to catch, not a widening of the old one.
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn no_digit_base64url_token_bound_to_secret_name_is_flagged() {
    // The other class veto #4 silenced: a random base64url token that happens to draw no digits
    // (0.9% of 24-char tokens) but does draw a `-`/`_`. 24 chars, no digits, one dash: measured
    // 108.0 total bits, above the 80-bit floor.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/vault.py",
        "session_token = 'kqZXvWyBpNdRtGmHsJfL-qwe'\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "high-entropy-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn identifier_shaped_and_placeholder_values_stay_silent_under_the_entropy_floor() {
    // The negatives the threshold measurement pinned (zzop_core::HIGH_ENTROPY_SECRET_MIN_BITS doc):
    // sentinel value==name (vetoed by hash equality), a short identifier value (41.4 bits), and the
    // decoy tree's hardest negative `PlaceholderSecretValue` (75.7 bits, 4.3 bits under the floor).
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/kinds.ts",
        concat!(
            "export const refresh_token = 'refresh_token';\n",
            "export const grantToken = 'refresh-token';\n",
            "export const secret = 'PlaceholderSecretValue';\n",
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "high-entropy-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mock_named_bindings_and_non_secret_names_stay_silent() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/fixtures.rs",
        concat!(
            "const MOCK_API_TOKEN: &str = \"correct-horse-battery-staple\";\n",
            "const CSS_CLASS: &str = \"mantine-DatePickerInput-input\";\n",
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "high-entropy-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The mock-word NAME veto must be boundary-anchored, not substring: `latestToken` embeds `test`
/// (la-TEST-…) and `attestationSecret` embeds `test` (at-TEST-ation), yet neither is a test
/// fixture name — a substring veto silenced BOTH on real secret-ending names (N1, measured). The
/// boundary set is judged for NAMES (start/`-`/`_`/camelCase transitions; no quote chars — names
/// carry none), mirroring `hardcoded-secret`'s boundary-anchored value-side veto.
#[test]
fn boundary_adjacent_names_are_not_silenced_by_the_mock_name_veto() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tokens.ts",
        concat!(
            "export const latestToken = 'correct-horse-battery-staple';\n",
            "export const attestationSecret = 'trombone_ravine_wallet_ember';\n",
        ),
    );
    let out = scan(&dir);
    let h = hits(&out, "high-entropy-secret");
    assert_eq!(h.len(), 2, "{:?}", out.findings);
    assert_eq!((h[0].line, h[1].line), (1, 2));
}

/// The veto's positives, across every boundary shape the anchored pattern claims: separator
/// (`test_secret`), name-start (`mockApiKey`), and camelCase interior (`myTestToken`) — all silent
/// even though every value clears the entropy floor and every name matches the secret-name gate.
#[test]
fn separator_start_and_camel_case_mock_names_stay_vetoed() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/fixture_names.ts",
        concat!(
            "export const test_secret = 'correct-horse-battery-staple';\n",
            "export const mockApiKey = 'correct-horse-battery-staple';\n",
            "export const myTestToken = 'correct-horse-battery-staple';\n",
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "high-entropy-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn high_entropy_secret_ok_marker_suppresses_across_comment_leaders() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.py",
        "# zzop-high-entropy-secret-ok: rotated fixture value\napi_key = 'correct-horse-battery-staple'\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "high-entropy-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- hardcoded-password (Java) ---

#[test]
fn direct_password_field_assignment_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Config.java",
        "public class Config {\n    public String password = \"sup3rSecretPwd\";\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "hardcoded-password");
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
        hits(&out, "hardcoded-password").len(),
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
        hits(&out, "hardcoded-password").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn java_pwd_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Config.java",
        "public class Config {\n    // zzop-hardcoded-password-ok: test fixture placeholder, rotated dummy value\n    public String password = \"sup3rSecretPwd\";\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-password").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- config-file-secret ---

#[test]
fn high_entropy_secret_in_a_properties_file_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/resources/application.properties",
        "spring.datasource.password=\njwt.secret=nRvyYC4soFxBdZ-F-5Nnzz5USXstR1YylsTd-mA0aKtI\njwt.sessionTime=86400\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "config-file-secret");
    assert_eq!(
        h.len(),
        1,
        "only jwt.secret should flag: {:?}",
        out.findings
    );
    assert_eq!(h[0].line, 2);
}

#[test]
fn empty_and_short_config_values_are_not_flagged() {
    // An empty `password=` and a short `password: root` dev value are below the 16-char floor.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/resources/application-dev.yml",
        "spring:\n  datasource:\n    password: root\n    username:\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "config-file-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn env_reference_config_value_is_not_flagged() {
    // `${JWT_SECRET}` is an environment reference, not a committed secret.
    let dir = TempDir::new("zzop-be-sec");
    dir.write("app.properties", "jwt.secret=${JWT_SECRET}\n");
    let out = scan(&dir);
    assert!(
        hits(&out, "config-file-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn secret_in_a_dotenv_file_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    // Split literal, same convention as `vendor_token_committed.rs`'s header -- see the AWS-key test
    // above for why a synthetic body still has to be split.
    dir.write(
        ".env",
        concat!("API_KEY=sk_li", "ve_abcdefghijklmnop0123\n"),
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "config-file-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn a_secret_in_a_code_file_is_not_a_config_file_secret() {
    // The config rule is scoped to config files; a `.ts` secret is `hardcoded-secret`'s job, not this rule's.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "const secret = \"abcd1234efgh5678ijkl\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "config-file-secret").is_empty(),
        "{:?}",
        out.findings
    );
}
