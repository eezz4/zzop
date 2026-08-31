use crate::{assert_disqualifier_summary_precedes_imperative, hits, scan, TempDir};

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
fn metadata_identifier_compared_to_a_string_literal_is_not_flagged() {
    // `tokenType === 'Bearer'` compares a metadata/scheme identifier (whose name merely contains a
    // secret-shaped word) against a PUBLIC string literal — no secret value on either side, so no
    // timing signal. Regression pin for the string-literal-RHS false-positive.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const tokenType: string;\nexport function isBearer() {\n  return tokenType === 'Bearer';\n}\n",
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
        "declare const token: string;\ndeclare const expectedToken: string;\nexport function checkToken() {\n  // zzop-timing-unsafe-compare-ok: token is a public request id, not a secret compared for auth\n  return token === expectedToken;\n}\n",
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

/// §27 pin (2026-08-28). This rule's remedy threw where it was applied, and the message did not say so.
///
/// It read: "Use `crypto.timingSafeEqual(...)` on fixed-length buffers instead." Applied literally to the
/// line this rule flags, that call does not return `false` — it THROWS, twice over, and both were measured
/// on node v22.22.3 rather than reasoned about:
///
///   * `crypto.timingSafeEqual('a', 'a')` -> `TypeError [ERR_INVALID_ARG_TYPE]`. Every corpus firing
///     compares two `string`s, so the literal substitution fails on the first request.
///   * `crypto.timingSafeEqual(Buffer.from('abc'), Buffer.from('abcd'))` ->
///     `RangeError [ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH]: Input buffers must have the same byte length`.
///     The flagged shape is a request-supplied value against a stored secret, where unequal length is the
///     NORMAL case, so a failed auth check becomes a 500 on every wrong guess.
///   * And the obvious guard does not hold: `'abcde'` and `'abcdé'` are both `.length === 5` and 5 vs 6
///     bytes, so an equal-`.length` pair still throws.
///
/// The message's own "on fixed-length buffers" named the precondition and gave the reader no way to meet
/// it. The remedy now does: hashing each side to a sha256 digest makes both operands 32 bytes for ANY
/// input (measured: `timingSafeEqual(h(''), h('a-very-long-secret-indeed'))` returns `false`), so the
/// throw is gone rather than documented.
///
/// The other half is who this reaches. Of 18 corpus firings, twelve are not two secret values at all —
/// enum state (`tokenStatus === TokenStatus.UNUSABLE_TOKEN_OBJECT`), a settings-form dirty check, a
/// browser-side array filter — because the rule keys on the identifier's NAME. Those readers needed a
/// question, not a longer list of exemptions, so the message asks whether a
/// caller submitting millions of guesses would learn anything from the line's timing, and says the
/// enumeration is examples.
///
/// Detection is untouched: every assertion above this comment is the pre-edit one.
#[test]
fn timing_compare_message_lands_the_throwing_remedy_before_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const token: string;\ndeclare const expectedToken: string;\nexport function checkToken() {\n  return token === expectedToken;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "timing-unsafe-compare");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
    let msg = &h[0].message;
    assert!(
        !msg.contains("Use `crypto.timingSafeEqual(...)` on fixed-length buffers instead."),
        "timing-unsafe-compare: the bare copyable remedy is back. Applied to the line this rule flags it \
         throws rather than returning false, which is the defect this pin exists for: {msg}"
    );
    assert_disqualifier_summary_precedes_imperative(
        "timing-unsafe-compare",
        msg,
        "TWO QUESTIONS DECIDE WHAT TO DO HERE",
        "hash each side to a fixed width and compare the digests",
        "ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH",
    );
    for needle in [
        "ERR_INVALID_ARG_TYPE",
        "counts UTF-16 code units",
        "examples rather than a list",
        "32 bytes on both sides",
    ] {
        assert!(
            msg.contains(needle),
            "timing-unsafe-compare: the landing lost {needle:?}: {msg}"
        );
    }
}

/// The remedy's OWN claim, measured rather than asserted: the message ends
/// "This rule judges only `===`/`!==`, so that rewrite clears the finding on its own." A prescription a
/// reader follows exactly and still sees flagged is how a channel gets switched off.
///
/// One factor differs between the two inputs — the comparison expression — and nothing else, so the
/// silence below is the rewrite's and not the fixture's.
#[test]
fn the_digest_rewrite_the_message_prescribes_actually_clears_the_finding() {
    let before = TempDir::new("zzop-be-sec");
    before.write(
        "api/auth.ts",
        "import { createHash, timingSafeEqual } from \"crypto\";\ndeclare const token: string;\ndeclare const expectedToken: string;\nconst h = (s: string) => createHash('sha256').update(s).digest();\nexport function checkToken() {\n  return token === expectedToken;\n}\n",
    );
    let out_before = scan(&before);
    assert_eq!(
        hits(&out_before, "timing-unsafe-compare").len(),
        1,
        "needle check: the fixture must fire BEFORE the rewrite, or the silence after it proves nothing. {:?}",
        out_before.findings
    );

    let after = TempDir::new("zzop-be-sec");
    after.write(
        "api/auth.ts",
        "import { createHash, timingSafeEqual } from \"crypto\";\ndeclare const token: string;\ndeclare const expectedToken: string;\nconst h = (s: string) => createHash('sha256').update(s).digest();\nexport function checkToken() {\n  return timingSafeEqual(h(token), h(expectedToken));\n}\n",
    );
    let out_after = scan(&after);
    assert!(
        hits(&out_after, "timing-unsafe-compare").is_empty(),
        "timing-unsafe-compare: the message promises this rewrite clears the finding, and it did not. {:?}",
        out_after.findings
    );
}
