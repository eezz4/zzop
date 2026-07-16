use crate::{hits, scan, TempDir};

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
