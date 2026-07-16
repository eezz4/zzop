use crate::{hits, scan, TempDir};

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
