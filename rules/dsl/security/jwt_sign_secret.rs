use crate::{hits, scan, TempDir};

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
fn this_rules_value_shape_veto_is_deliberately_narrower_than_hardcoded_secrets() {
    // T3 DIVERGENCE PIN (policy-value inventory, rule-quality.md §6). `hardcoded-secret` folded its two
    // casing-specific identifier arms into one casing-AGNOSTIC arm after seven measured false positives
    // landed on it. This rule kept the old narrow pair (all-lowercase snake / all-lowercase kebab)
    // ON PURPOSE: no measurement has arrived for the positional `jwt.sign` form, and widening a veto with
    // zero measurement buys silence, not signal — on a CRITICAL rule whose finding means "anyone can forge
    // a token". The same value therefore gets two different verdicts, which is the divergence made
    // visible: `apiKey: "Mantine_DatePicker_Input"` is silenced by hardcoded-secret's folded arm, while
    // the identical literal handed to `jwt.sign` still fires here (correctly — an identifier-shaped
    // signing key IS a forgeable signing key). If a measurement later says to fold this arm too, update
    // the inventory row in the same change.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        concat!(
            "declare const jwt: any;\n",
            "declare const payload: any;\n",
            "export const cfg = { apiKey: \"Mantine_DatePicker_Input\" };\n",
            "export function issue() {\n",
            "  return jwt.sign(payload, \"Mantine_DatePicker_Input\");\n",
            "}\n",
        ),
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "jwt-sign-literal-secret").len(),
        1,
        "{:?}",
        out.findings
    );
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn jwt_secret_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issue() {\n  // zzop-jwt-sign-literal-secret-ok: rotated test-only fixture key, not a real credential\n  return jwt.sign(payload, \"abcd1234efgh5678\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-sign-literal-secret").is_empty(),
        "{:?}",
        out.findings
    );
}
