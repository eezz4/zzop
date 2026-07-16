use crate::{hits, scan, TempDir};

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
