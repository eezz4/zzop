use crate::{
    assert_disqualifier_summary_precedes_imperative, hits, scan, scan_with_secret_names, TempDir,
};

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

/// §27 pin (2026-08-29). This rule DISCLOSES its own worst false positive and, until this commit, put
/// it 2.7 KB behind the rotation prescription: an English-word identifier chain that clears the 80-bit
/// floor (`"mantine-DatePickerInput-input"`, 107.4 bits) is indistinguishable from a passphrase by any
/// content-free statistic, so bound to a secret-ending name it FIRES. The reader who acts on the first
/// instruction -- BEFORE ROTATING, FIND WHAT IS STILL USING THIS VALUE -- starts inventorying the
/// consumers of a value that may not be a credential at all, and reaches the sentence saying so only
/// after the cutover is already under way.
///
/// WHAT MOVED, and why it was the block and not the clause: the clause cannot be lifted. It is one arm
/// of a semicolon list ("Remaining blindness ...: a WEAK sub-floor credential ...; an English-word
/// identifier chain ...; and 3-word passphrases ..."), and pulling one arm out of a list is punctuation
/// surgery, not a move. So the ROTATION BLOCK travelled instead -- "A credential committed to source
/// ..." through "... a secrets manager.", 946 characters -- and it had to travel whole for a second
/// reason: it CONTAINS `rotation_landing.rs::ROTATION_LANDING`, a byte-identical constant that eight
/// rules share, so the insertion point was forced rather than chosen. Message length 6748 before and
/// after with the character multiset identical: the repair is a permutation, which settles "does §27
/// grow messages?" for this rule by construction rather than by measurement.
///
/// The right-hand anchor here is the LANDING's opening sentence, not the rotate imperative, and that is
/// the point. `rotation_landing.rs::high_entropy_secret_landing_precedes_the_imperative` already pins
/// `landing < Rotate/revoke ...`; a second pin on that same needle would assert nothing new. Anchored
/// one link earlier the two compose into disqualifier < landing < imperative, and each still fails
/// alone.
///
/// INVALIDATION PROBE: restore the HEAD order (rotation block back in front of "This rule reads the
/// string-literal-with-binding-name IR node"). Every token stays present and spelled exactly once, a
/// `contains` pin stays green, and this assertion alone goes red.
#[test]
fn high_entropy_secret_disqualifier_precedes_the_rotation_landing() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const dbPassword = 'correct-horse-battery-staple';\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "high-entropy-secret");
    // Sentence order only -- the finding itself is unchanged.
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
    assert_disqualifier_summary_precedes_imperative(
        "high-entropy-secret",
        &h[0].message,
        "an English-word identifier chain long enough to clear the floor",
        "BEFORE ROTATING, FIND WHAT IS STILL USING THIS VALUE",
        "so when bound to a secret-ending name it fires",
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

/// The shape that made this rule PRESCRIPTION-BREAKING: in `addCriterion("password =", value,
/// "password")` the matched `password =` lives INSIDE a string literal — it is a SQL column-operator
/// fragment — and the `"` the assign pattern reads as the value's OPENING delimiter is in fact that
/// fragment's CLOSING one. What it captured as "the hardcoded password" was the argument text
/// `, value, ` sitting BETWEEN two literals; the real value is the identifier `value`. Following the
/// message (lift the literal into config/env) breaks the query.
///
/// This was the WHOLE corpus firing set. Measured 2026-08-25 with this rule's own two needles over
/// every Java tree in both corpora — 10 trees, 5,763 `.java` files — exactly TWO candidate lines exist,
/// and both are this line: `mall-mbg/.../UmsAdminExample.java:249` and `.../UmsMemberExample.java:336`.
/// There is no true positive for this rule anywhere in the corpus, which is why the preserved side is
/// priced by the PLANTED credential below rather than by a count.
///
/// That planted credential is the discriminating half, and its placement is deliberate: it sits in the
/// SAME file, on a `…/model/*Example.java` path inside a MyBatis-generated model package, and is
/// asserted BY LINE. A generated-file banner veto, a path veto, or a filename veto would all buy the
/// silence above while being green here — so those are the repairs this test is built to fail. (The
/// banner veto is not merely weaker, it is unavailable: `mall-mbg`'s generated models carry NO head
/// comment at all — line 1 is `package com.macro.mall.model;` — so there is no declaration of
/// provenance in the scanned source to stand on. The declaration this repair does stand on is the
/// file's own quote delimiters, which say `password =` is string data and not an assignment.)
///
/// The 4-character value on the last line is not filler: it pins that the `{4,}` floor is still
/// measured against the value's real length, which holds only because the masking is
/// length-preserving (`zzop_core::dsl::string_mask`). A masker that deleted literals instead of
/// blanking them would drop that line and this assertion would catch it.
#[test]
fn a_sql_operator_fragment_is_not_an_assignment_and_a_planted_password_beside_it_still_fires() {
    let dir = TempDir::new("zzop-be-pwd-mbg");
    dir.write(
        "mall-mbg/src/main/java/com/macro/mall/model/UmsAdminExample.java",
        concat!(
            "package com.macro.mall.model;\n",
            "\n",
            "public class UmsAdminExample {\n",
            "    public Criteria andPasswordEqualTo(String value) {\n",
            "        addCriterion(\"password =\", value, \"password\");\n",
            "        return (Criteria) this;\n",
            "    }\n",
            "\n",
            "    public String password = \"Pr0dOnly!N3verRotated\";\n",
            "    public String pwd = \"abcd\";\n",
            "}\n",
        ),
    );
    let out = scan(&dir);
    let lines: Vec<u32> = hits(&out, "hardcoded-password")
        .iter()
        .map(|h| h.line)
        .collect();
    assert_eq!(
        lines,
        vec![9, 10],
        "a SQL operator fragment must not read as an assignment, and a real credential in the same \
         generated file must still fire: {:?}",
        out.findings
    );
}

/// The same defect one spelling away from the measured one, and outside MyBatis entirely: a SQL clause
/// built by concatenation puts `password = ` at the END of a literal, so the closing quote again reads
/// as an opening one and the "value" is the concatenation operators. Kept beside the MyBatis fixture
/// because a repair fitted to `addCriterion(` — or to `Example.java`, or to `mall-mbg/` — passes that
/// test and fails this one; the corpus contains only the one tree, so target-fitting is otherwise
/// invisible.
///
/// The JDBC arm rides in the SAME file and is asserted by line: masking blanks literal INTERIORS but
/// keeps the delimiters, so `"[^"]+"` still matches three blanked arguments. A change that bought the
/// silence above by weakening the file pattern, dropping the arm, or masking the delimiters too would
/// be green on an `is_empty()` assertion and is red here.
#[test]
fn a_concatenated_sql_password_clause_is_silent_and_the_jdbc_arm_still_fires() {
    let dir = TempDir::new("zzop-be-pwd-concat");
    dir.write(
        "src/main/java/com/example/db/UserDao.java",
        concat!(
            "public class UserDao {\n",
            "    String q = \"UPDATE users SET password = \" + quote(v) + \" WHERE id = 1\";\n",
            "    Connection open() throws Exception {\n",
            "        return DriverManager.getConnection(\"jdbc:mysql://db/app\", \"admin\", \"p@ssw0rd\");\n",
            "    }\n",
            "}\n",
        ),
    );
    let out = scan(&dir);
    let lines: Vec<u32> = hits(&out, "hardcoded-password")
        .iter()
        .map(|h| h.line)
        .collect();
    assert_eq!(
        lines,
        vec![4],
        "a concatenated SQL clause is not an assignment, and the JDBC arm must survive masking: {:?}",
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

#[test]
fn a_password_containing_punctuation_is_seen_like_one_without() {
    // The value class was `[A-Za-z0-9+/_-]` until 2026-08-16, so a single `!` or `#` turned this rule
    // off and its sensitivity ran BACKWARDS: the weaker a password was, the likelier it got reported.
    // Both spellings of the same credential are asserted in one test because the defect was only ever
    // visible as the CONTRAST between them — either alone looks like ordinary behaviour.
    let dir = TempDir::new("zzop-be-sec-punct");
    dir.write(
        "api/config.ts",
        concat!(
            "export const A_PASSWORD = \"Pr0d!Postgres#2024\";\n",
            "export const B_PASSWORD = \"Pr0dPostgres2024\";\n",
            "export const C_SECRET = \"P@ssw0rd!Complex#2024\";\n"
        ),
    );
    let out = scan(&dir);
    let mut lines: Vec<u32> = hits(&out, "hardcoded-secret")
        .iter()
        .map(|h| h.line)
        .collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        vec![1, 2, 3],
        "punctuation must not hide a credential: {:?}",
        out.findings
    );
}

#[test]
fn a_dot_joined_placeholder_is_still_an_identifier_shape() {
    // The veto that keeps `refresh-token` and `MY_API_KEY` out has to cover `jwt.token.here` too: when
    // the value class above was widened, that one placeholder produced all 19 of the new findings
    // measured on corpus/oss, every one in a test file. The dot is the same separator idea as `-`/`_`,
    // so it joined that arm rather than the widening being rolled back. A value whose segments carry
    // digits or punctuation is NOT this shape and keeps firing — asserted beside it so a later
    // tightening of the veto cannot quietly take real credentials with it.
    let dir = TempDir::new("zzop-be-sec-dotted");
    dir.write(
        "api/fixtures.ts",
        concat!(
            "export const token = \"jwt.token.here\";\n",
            "export const secret = \"v1.a9F2!kQ\";\n"
        ),
    );
    let out = scan(&dir);
    let lines: Vec<u32> = hits(&out, "hardcoded-secret")
        .iter()
        .map(|h| h.line)
        .collect();
    assert_eq!(
        lines,
        vec![2],
        "the dotted word-placeholder must be vetoed and the dotted credential kept: {:?}",
        out.findings
    );
}

// --- `SECRET_KEY`: the name arms match a keyword the identifier ENDS with, so `secret` never reached
// --- a name that CONTINUES past it (2026-08-18)

/// Django/Flask/Rails all commit their signing key as `SECRET_KEY = "..."`, and both secret rules were
/// blind to it: the line scan's name arm wants the keyword immediately before the `=`, and the literal
/// scan's `name_pattern` is anchored with `$`, so `SECRET_KEY` ended in `KEY` and matched neither. Found
/// by measurement, not by reading — `be-django/conduit/settings.py:23` in corpus/oss carries a real
/// Django secret key and the whole file reported ZERO findings from ANY rule before this fixture.
///
/// Both rules are asserted here on purpose: they reach the same line by different routes (value shape
/// vs. entropy), and a later edit that fixes only one would otherwise leave the other silent.
#[test]
fn django_style_secret_key_is_flagged_by_both_secret_rules() {
    let dir = TempDir::new("zzop-be-sec-skey");
    dir.write(
        "conf/settings.py",
        "SECRET_KEY = '2^f+3@v7$v1f8yt0!s)3-1t$)tlp+xm17=*g))_xoi&&9m#2a&'\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "line scan must reach a name that continues past the keyword: {:?}",
        out.findings
    );
    assert_eq!(
        hits(&out, "high-entropy-secret").len(),
        1,
        "literal scan must reach it too: {:?}",
        out.findings
    );
}

/// U103's drill, closed. A project that declares its OWN secret names is judged by them.
///
/// Measured 2026-08-18, before this channel existed: declaring six names and planting one line each
/// produced three findings, and the three that were silently ignored got no `configWarnings` entry
/// either. The user had said "these are my secret names" and the tool answered with its own list.
///
/// The second half of this test is the direction that surprises, and it is the contract rather than a
/// side effect: `PASSWORD` — a built-in — STOPS being judged, because this key REPLACES. See
/// `RulePackDef::rewrite_secret_names` for the failure-direction test that puts it on that side of the
/// line, and note what it costs: a config that declares a narrow list narrows the rule. That is why
/// `zzop init` writes the built-in ten, and why deleting the key is a decision rather than a tidy-up.
#[test]
fn a_declared_vocabulary_replaces_the_built_in_secret_names() {
    let dir = TempDir::new("zzop-be-sec-declared");
    dir.write(
        "app/keys.ts",
        concat!(
            "export const JWT = \"eyJhbGciOiJIUzI1NiJ9xxxxxxxx\";\n",
            "export const NONCE = \"9f8e7d6c5b4a39281706\";\n",
            "export const PASSWORD = \"hunter2hunter2hunter2\";\n"
        ),
    );

    let declared = scan_with_secret_names(&dir, Some(&["jwt", "nonce"]));
    let lines: Vec<u32> = hits(&declared, "hardcoded-secret")
        .iter()
        .map(|h| h.line)
        .collect();
    assert_eq!(
        lines,
        vec![1, 2],
        "the two DECLARED names must fire and the undeclared built-in must not: {:?}",
        declared.findings
    );

    // The control: under the shipped default the same file reports the opposite line, so this test
    // cannot pass because the rule went quiet or because the fixture stopped parsing.
    let default = scan(&dir);
    let default_lines: Vec<u32> = hits(&default, "hardcoded-secret")
        .iter()
        .map(|h| h.line)
        .collect();
    assert_eq!(
        default_lines,
        vec![3],
        "under the built-in vocabulary only `PASSWORD` is a secret name: {:?}",
        default.findings
    );
}

/// A config that omits the key makes NO name judgment — the standing contract for every vocabulary key
/// in `zzop_engine::VocabularyConfig` except `extraTestPathPatterns`, applied here.
///
/// It is pinned in its own test because the failure mode is silence, and silence is what every other
/// test in this file is unable to distinguish from a fixture that stopped producing secrets. The
/// assertion below is therefore paired: the same bytes MUST still fire under a declaration.
#[test]
fn an_undeclared_vocabulary_makes_no_secret_name_judgment_at_all() {
    let dir = TempDir::new("zzop-be-sec-undeclared");
    dir.write(
        "app/keys.ts",
        "export const PASSWORD = \"hunter2hunter2hunter2\";\n",
    );
    assert!(
        hits(&scan_with_secret_names(&dir, None), "hardcoded-secret").is_empty(),
        "an undeclared vocabulary must not judge any name"
    );
    assert_eq!(
        hits(&scan_with_secret_names(&dir, Some(&["password"])), "hardcoded-secret").len(),
        1,
        "the same bytes under a declaration — without this the test above passes on a broken fixture"
    );
}

/// The alternation this pack spells inline is the one `zzop_core::dsl::secret_names` owns, in exactly
/// the number of arms that module declares.
///
/// This is the whole safety of the rewrite seam. `${NAME}` fragments substitute only as a WHOLE value
/// and the alternation sits mid-pattern, so the pack carries a COPY of the text — and a copy nobody
/// counts is a copy that drifts. Reword one arm and this fails here, loudly, instead of that arm
/// silently ceasing to receive a project's declaration with every other test still green.
#[test]
fn the_pack_spells_the_owned_alternation_in_exactly_the_declared_number_of_arms() {
    let source = include_str!("security.json");
    let arms = source
        .matches(&zzop_core::dsl::secret_names::group())
        .count();
    assert_eq!(
        arms,
        zzop_core::dsl::secret_names::PACK_ARMS,
        "security.json carries the owned secret-name alternation in {arms} arm(s), and \
         `secret_names::PACK_ARMS` says {}. Either a rule was reworded (and has stopped receiving \
         `vocabulary.secretNames`) or a new arm was added (and nobody decided whether it should be \
         declarable). Both are triage, neither is a number to update.",
        zzop_core::dsl::secret_names::PACK_ARMS
    );
}

/// The other half of the same decision, and the reason it is `secret[_-]?key` rather than a bare `key`
/// in the name list. Adding `key` catches the fixture above as well — and over the 17-repo corpus it
/// brought 46 further findings of which exactly ONE was a credential. These three lines are the shapes
/// that produced the rest, kept as a standing negative so the next widening has to run this first.
#[test]
fn bare_key_named_bindings_are_not_secrets() {
    let dir = TempDir::new("zzop-be-sec-barekey");
    dir.write(
        "app/flags.java",
        concat!(
            "  private static final String DECIDER_KEY = \"enable_very_recent_tweets\";\n",
            "  public static final String HEADER_KEY = \"Authorization\";\n",
            "  public static final String NAMED_CLIENT_QUOTA_KEY = \"clientQuotaKey\";\n",
            // The real credential rides in the SAME file as the three near-misses, deliberately: a
            // negative that stands alone proves the rules are quiet, not that they are awake, so a
            // change that buys silence by dropping `.java` from the file patterns would pass it.
            // With both here, that change fails on this line instead.
            "  public static final String SECRET_KEY = \"2^f+3@v7$v1f8yt0!s)3-1t$\";\n"
        ),
    );
    let out = scan(&dir);
    let line_hits: Vec<u32> = hits(&out, "hardcoded-secret")
        .iter()
        .map(|h| h.line)
        .collect();
    assert_eq!(
        line_hits,
        vec![4],
        "flag/header/binding names are not credentials, and the real one must still fire: {:?}",
        out.findings
    );
    // This test deliberately judges ONE rule, and the sibling's absence is the reason. Both secret
    // rules declare `.java` in their `file_pattern`, but `high-entropy-secret` reads the
    // string-literal-with-binding-name IR channel, which has no Java producer today — probed
    // 2026-08-18 with a 44-char high-entropy value on this same fixture and it stayed silent, so its
    // `.java` claim is a file-pattern that its substrate cannot deliver. Naming it here would put it
    // in `java_lane_evidence.rs`'s "named by a Java unit test" census on the strength of a silence
    // that proves nothing about Java, and catalog.md's ladder would then say it is Java-covered.
    // Its firing/near-miss pair lives on the Python fixture above, which is where it can actually see.
}

// --- config-file-secret: the value no longer has to end its line (2026-08-20) ---

#[test]
fn a_config_secret_with_a_trailing_comment_is_flagged_like_one_without() {
    // The pattern ended `["']?\s*$` until 2026-08-20, so ONE trailing comment turned the rule off.
    // Both spellings of the same credential are asserted in one test because the defect was only ever
    // visible as the CONTRAST between them -- either line alone looks like ordinary behaviour.
    // Measured on macrozheng/mall, which writes every committed secret with a trailing comment: the
    // whole repo reported ZERO config secrets while carrying two JWT signing keys.
    let dir = TempDir::new("zzop-be-sec-cfgcomment");
    dir.write(
        "src/main/resources/application.yml",
        concat!(
            "jwt:\n",
            "  secret: mall-admin-secret-value #JWT signing key\n",
            "  token: mall-admin-secret-value\n",
            "  password: mall-admin-secret-value ; ini-style trailing comment\n",
        ),
    );
    let out = scan(&dir);
    let mut lines: Vec<u32> = hits(&out, "config-file-secret")
        .iter()
        .map(|h| h.line)
        .collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        vec![2, 3, 4],
        "a trailing comment must not hide a committed secret: {:?}",
        out.findings
    );
}

#[test]
fn an_empty_config_value_followed_by_a_comment_is_still_not_flagged() {
    // The comment tail admits a comment AFTER a value, never INSTEAD of one -- `password:` with only a
    // comment after it is the single commonest line in the corpus that motivated the widening.
    let dir = TempDir::new("zzop-be-sec-cfgempty");
    dir.write(
        "src/main/resources/application-dev.yml",
        "spring:\n  redis:\n    password: # empty by default\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "config-file-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_secret_in_an_appsettings_json_is_flagged() {
    // `.json` was unreachable for this rule until 2026-08-20 even though `appsettings.json` IS the
    // .NET configuration format. Two things had to move, not one: the `file_pattern` had to admit
    // `.json`, AND the end-of-line anchor had to go -- a JSON value never ends its own line.
    let dir = TempDir::new("zzop-be-sec-appsettings");
    dir.write(
        "src/Catalog.API/appsettings.json",
        concat!(
            "{\n",
            "  \"ConnectionStrings\": { \"Catalog\": \"Host=db;Password=SuperSecretPw123456\" },\n",
            "  \"ApiKey\": \"9f3Kd0Lm2Qr7Tz4Xb8Vn\"\n",
            "}\n",
        ),
    );
    let out = scan(&dir);
    let mut lines: Vec<u32> = hits(&out, "config-file-secret")
        .iter()
        .map(|h| h.line)
        .collect();
    lines.sort_unstable();
    assert_eq!(lines, vec![2, 3], "{:?}", out.findings);
}

#[test]
fn a_placeholder_value_in_a_config_file_is_not_flagged() {
    // dotnet/eShop ships `Password=yourWeak(!)Password` in five `appsettings.Development.json` files.
    // Admitting `.json` made all five reachable at once, and none of them is a credential -- so the
    // veto grew a VALUE-POSITION arm rather than the file type being kept out. Judged on the value,
    // not the line: a placeholder word anywhere on the line would veto a real secret sitting beside a
    // comment that happens to say "example".
    //
    // A REAL SECRET SITS IN THE SAME FILE and is asserted by LINE, which is what makes this a test of
    // the veto rather than of the rule's pulse. Asserting only `is_empty()` here was green in three
    // ways that have nothing to do with placeholders: `.json` falling back out of the `file_pattern`,
    // the rule failing to load, and the whole pack going dark all produce the same empty vector. The
    // shape is `scan_scope.rs`'s `a_template_attribute_binding_in_an_sfc_is_not_a_credential` -- one
    // scan, one `assert_eq!` on the firing lines, so a silence claim cannot outlive its subject.
    let dir = TempDir::new("zzop-be-sec-cfgplaceholder");
    dir.write(
        "src/Catalog.API/appsettings.Development.json",
        concat!(
            "{\n",
            "  \"ConnectionStrings\": { \"Catalog\": \"Host=localhost;Password=yourWeak(!)Password\" },\n",
            "  \"Token\": \"test-account-token-value\",\n",
            "  \"ApiKey\": \"9f3Kd0Lm2Qr7Tz4Xb8Vn\"\n",
            "}\n",
        ),
    );
    let out = scan(&dir);
    let mut lines: Vec<u32> = hits(&out, "config-file-secret")
        .iter()
        .map(|h| h.line)
        .collect();
    lines.sort_unstable();
    assert_eq!(lines, vec![4], "{:?}", out.findings);
}

#[test]
fn a_secret_key_named_config_entry_is_flagged() {
    // `secret` matched only when a `=`/`:` FOLLOWED it, so `SECRET_KEY=` reached neither arm -- the
    // same shape that made a committed Django SECRET_KEY invisible to both source-literal secret rules
    // until 2026-08-18. `secret[_-]?key` is spelled here for the same reason it is spelled there.
    let dir = TempDir::new("zzop-be-sec-secretkey");
    dir.write(".env", "SECRET_KEY=k7Jx2pQw9Zr4Tn6Vb8Ly0Mc3Df5Gh1\n");
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "config-file-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn a_short_config_password_is_still_below_the_floor() {
    // The 16-char floor is KEPT, and this test is its disclosure rather than its endorsement: these
    // three lines are real committed credentials in macrozheng/mall and this rule reports none of
    // them. Measured 2026-08-20: dropping the floor to 6 over mall/eShop/koel adds 14 lines, 8 real
    // and 6 ephemeral CI-service passwords, and nothing in a LENGTH test can separate the two. If a
    // later change lowers the floor, this test is the one that has to be rewritten deliberately.
    let dir = TempDir::new("zzop-be-sec-cfgfloor");
    dir.write(
        "src/main/resources/application-prod.yml",
        concat!(
            "spring:\n",
            "  datasource:\n",
            "    password: 123456\n",
            "minio:\n",
            "  accessKey: minioadmin #access key\n",
            "  secretKey: minioadmin #secret key\n",
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "config-file-secret").is_empty(),
        "the floor's blind spot is documented, not accidental: {:?}",
        out.findings
    );
}

// --- config-file-secret: TRANSLATION CATALOGUES (2026-08-26) ---
//
// The value gate is "16+ non-space, non-quote characters", which is a proxy for "looks like a secret"
// and collapses in writing systems that do not separate words with spaces. Measured on cal.com
// (`176037d`): 59 findings, 50 of them UI labels in `packages/i18n/locales/<lang>/common.json`, and the
// language distribution was km 24 / ja 14 / zh 5 / de 2 / fi 2 / da,no,sv 1 each -- the defect lands on
// non-Latin scripts and on compounding languages, not evenly. immich (`i18n/<lang>.json`) added 17 and
// grafana (`public/locales/<lang>/*.json`) 20; every one of those 87 was read and none was a credential.
//
// The exclusion's evidence is the AUTHOR'S OWN DECLARATION, which is what a suppression needs: the file
// sits under a `locales/`/`locale/`/`i18n/` directory AND the next path component is a BCP-47 language
// tag (a directory, as in cal.com and grafana, or the file's stem, as in immich). Both halves are
// required. The container alone is not enough -- `packages/i18n/package.json` is not a catalogue -- and
// the tag alone is not enough, which is what the `lang/no.json` and `config/no/` fixtures below pin:
// `no` is Norwegian AND an ordinary directory name, and so are `id`, `is`, `it` and `be`.

#[test]
fn a_translated_label_in_a_locale_directory_is_not_a_committed_secret() {
    // Verbatim from cal.com `packages/i18n/locales/da/common.json:1129` -- Danish for "client secret",
    // 17 characters with no space in it, which is the entire reason the rule saw a credential.
    let dir = TempDir::new("zzop-be-sec-locale");
    dir.write(
        "packages/i18n/locales/da/common.json",
        "{\n  \"client_secret\": \"Klienthemmelighed\",\n  \"other\": \"x\"\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "config-file-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_translation_catalogue_named_by_its_stem_is_not_a_committed_secret() {
    // immich's shape (`i18n/<tag>.json`), and the three tag spellings the corpus actually carries
    // beyond a bare two-letter code: a region (`nb_NO`), a script (`zh-Hans`) and a three-letter code
    // (`fil`). Each is a separate `common.json`-sized catalogue in the real trees; one line each here.
    let dir = TempDir::new("zzop-be-sec-locale-stem");
    for rel in [
        "i18n/th.json",
        "i18n/nb_NO.json",
        "i18n/zh-Hans.json",
        "i18n/fil.json",
    ] {
        dir.write(
            rel,
            "{\n  \"admin_password\": \"Administratorpassord\"\n}\n",
        );
    }
    let out = scan(&dir);
    assert!(
        hits(&out, "config-file-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_config_file_under_a_locales_directory_that_is_not_a_language_tag_still_reports() {
    // The fence on the tag half. A file sitting DIRECTLY under `locales/` whose name is not a language
    // tag is not a catalogue, and neither is a sibling of the `locales/` directory. Without the tag
    // requirement, exempting the container would take both of these with it.
    let dir = TempDir::new("zzop-be-sec-locale-fence");
    dir.write(
        "packages/i18n/locales/config.json",
        "{\n  \"jwt_secret\": \"nRvyYC4soFxBdZF5Nnzz5USXstR1Yyl\"\n}\n",
    );
    dir.write(
        "packages/i18n/settings.yml",
        "jwt:\n  secret: nRvyYC4soFxBdZF5Nnzz5USXstR1Yyl\n",
    );
    let mut lines: Vec<u32> = hits(&scan(&dir), "config-file-secret")
        .iter()
        .map(|f| f.line)
        .collect();
    lines.sort_unstable();
    assert_eq!(lines, vec![2, 2], "both files must still be judged");
}

#[test]
fn a_language_code_directory_outside_a_locales_container_still_reports() {
    // `no` is Norwegian and it is also a perfectly ordinary directory name. The exclusion is anchored on
    // the i18n container, so a language-shaped path segment anywhere else buys nothing -- this fixture
    // is the negative half of the previous test and the one that goes red if the container half is ever
    // dropped in favour of "the path contains a language code".
    let dir = TempDir::new("zzop-be-sec-lang-collision");
    dir.write(
        "deploy/no/application.yml",
        "spring:\n  datasource:\n    password: Pr0dPgPassw0rd2024x\n",
    );
    dir.write(
        "db/id/application.yml",
        "spring:\n  datasource:\n    password: Pr0dPgPassw0rd2024x\n",
    );
    let mut lines: Vec<u32> = hits(&scan(&dir), "config-file-secret")
        .iter()
        .map(|f| f.line)
        .collect();
    lines.sort_unstable();
    assert_eq!(lines, vec![3, 3], "{:?}", scan(&dir).findings);
}

#[test]
fn a_lang_or_translations_directory_is_deliberately_not_exempt() {
    // The two container words the corpus carries that were MEASURED and DECLINED, because widening to a
    // direction that harvests nothing buys only risk: nocodb's `packages/nc-gui/lang/<tag>.json` is 40
    // files and 0 findings, grafana's `apps/*/pkg/translations/<tag>/*.json` is 19 files and 0 findings,
    // and spring-petclinic's ResourceBundle `messages/messages_<tag>.properties` is 9 files and 0
    // findings. If a later measurement finds real translation false positives under one of those, this
    // test is the one that has to be rewritten deliberately -- recount first with:
    //   zzop analyze --config <tree>.jsonc --rule security/config-file-secret --limit 1000
    let dir = TempDir::new("zzop-be-sec-lang-declined");
    dir.write(
        "packages/nc-gui/lang/no.json",
        "{\n  \"client_secret\": \"Klienthemmelighet\"\n}\n",
    );
    dir.write(
        "apps/advisor/pkg/translations/de-DE/advisor.json",
        "{\n  \"client_secret\": \"Klientgeheimnis1\"\n}\n",
    );
    let mut lines: Vec<u32> = hits(&scan(&dir), "config-file-secret")
        .iter()
        .map(|f| f.line)
        .collect();
    lines.sort_unstable();
    assert_eq!(lines, vec![2, 2], "{:?}", scan(&dir).findings);
}

/// F7-1 (external review, 2026-09-04): this rule's message said the name arms "match a keyword that
/// the identifier ENDS with". They do not. The arm requires the keyword at a WORD BOUNDARY — preceded
/// by a non-alphanumeric character or starting the line — which is the opposite end of the identifier.
/// So `clientSecret` and `myPassword`, the dominant camelCase spellings in TypeScript, were silent
/// while the shipped sentence claimed they were covered. A rule that misstates its own reach in its own
/// message is the "false self-report" class this project keeps as a 1.0 veto, so the sentence was the
/// defect and it was repaired rather than the regex — widening the regex is a DETECTION change and
/// would need its own measurement and gate run.
///
/// The pin asserts the BEHAVIOUR both directions, not the prose: prose is checked by the message-token
/// guards, and a pin that only read the sentence would go green on a sentence that lies consistently.
/// The sibling half matters as much as the miss: `security/high-entropy-secret` anchors its
/// binding-name test to the END of the name, so it DOES report `clientSecret` once the value clears its
/// 80-bit floor. That is why the corrected sentence names exactly two residual blind spots rather than
/// claiming the pair is closed — and this test pins the sibling's catch, so a future narrowing there
/// turns the corrected sentence back into a false one and goes red here first.
#[test]
fn the_name_arms_match_at_a_word_boundary_and_the_sibling_covers_the_trailing_keyword() {
    let dir = TempDir::new("zzop-sec-name-boundary");
    // One value, four spellings: only the NAME shape differs, so nothing else can explain the split.
    dir.write(
        "src/a.ts",
        "const clientSecret = \"aB3xK9mQ7zP2wL5nR8tV4yH6jF1dG0sC\";\n\
         const myPassword = \"aB3xK9mQ7zP2wL5nR8tV4yH6jF1dG0sC\";\n\
         const client_secret = \"aB3xK9mQ7zP2wL5nR8tV4yH6jF1dG0sC\";\n\
         const apiKey = \"aB3xK9mQ7zP2wL5nR8tV4yH6jF1dG0sC\";\n",
    );
    let out = scan(&dir);
    let line_scan: Vec<u32> = hits(&out, "hardcoded-secret")
        .iter()
        .map(|f| f.line)
        .collect();
    assert_eq!(
        line_scan,
        vec![3, 4],
        "the boundary arm reaches `client_secret` (line 3) and `apiKey` (line 4) and NOT the camelCase \
         spellings on lines 1-2. If lines 1-2 appear here the regex was widened — then the corrected \
         message paragraph is stale and must be rewritten in the same change: {:?}",
        out.findings
    );
    let sibling: Vec<u32> = hits(&out, "high-entropy-secret")
        .iter()
        .map(|f| f.line)
        .collect();
    assert!(
        sibling.contains(&1) && sibling.contains(&2),
        "the sibling anchors to the END of the binding name, which is what makes the camelCase gap \
         narrow rather than total — the corrected message says so, and it is only true while this \
         holds: {:?}",
        out.findings
    );
}
