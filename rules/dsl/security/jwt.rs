use crate::{
    assert_disqualifier_summary_precedes_imperative, assert_landing_precedes_imperative, hits,
    scan, TempDir,
};

/// The circulation landing for `jwt-none-algorithm`, spliced ahead of the "remove `none`" imperative.
///
/// WHY (`1.architecture/rules/rule-quality.md` §27 leg 3). Its two siblings in this file already carry
/// this class of sentence — `jwt-no-expiry` and `jwt-sign-literal-secret` both tell the reader what
/// happens to tokens already in circulation — and the `critical` of the three, with the same blast
/// radius and the same shape of fix, carried nothing. `alg` is a header parameter inside the token's
/// own signed-over bytes, so narrowing the verifier re-issues nothing: it rejects at deploy every
/// token a browser, a mobile client, a cron job or a sibling service is already holding.
///
/// WHY THIS ONE CANNOT BORROW `ROTATION_LANDING`, one file over. That constant's whole second half is
/// an overlapping window — migrate the holders, then retire the old value — and there is no window
/// here that is not itself the vulnerability: accepting `alg: none` for one more request is the defect.
/// So this landing says the opposite of its sibling's on purpose: the sessions END, and what gets
/// sequenced is the CLIENTS and any sibling still minting `none`, not the acceptance window.
///
/// NOT A DISQUALIFIER. The finding is right and the edit is right; the landing prices it.
///
/// POSITION, not presence: the invalidation probe is to move this constant behind the imperative with
/// every token still spelled exactly once.
const TOKEN_CIRCULATION_LANDING: &str = "EVERY TOKEN ALREADY IN CIRCULATION WAS MINTED UNDER THE OLD LIST, AND THIS EDIT IS WHAT STOPS ACCEPTING THEM";

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

#[test]
fn jwt_sign_with_an_exp_payload_claim_is_not_flagged() {
    // A token that expires via the `exp` payload claim (not the `expiresIn` option) still expires — the
    // rule's own message endorses "or set `exp` in the payload". Regression pin (dogfood: corpus be-nest,
    // user.service.ts generateJWT sets `exp: today.getTime() / 1000`).
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\nexport function issueToken(id: number) {\n  const exp = new Date();\n  exp.setDate(exp.getDate() + 60);\n  return jwt.sign({ id, exp: exp.getTime() / 1000 }, \"secret\");\n}\n",
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

/// POSITION pin on a DELIVERED finding (§27 leg 3): what removing `none` does to tokens already held
/// is reached before the instruction to remove it.
#[test]
fn the_circulation_landing_precedes_the_remove_none_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import jwt from \"jsonwebtoken\";\nexport function verify(token: string) {\n  return jwt.verify(token, \"\", { algorithms: [\"none\"] });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "jwt-none-algorithm");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "jwt-none-algorithm",
        &h[0].message,
        TOKEN_CIRCULATION_LANDING,
        "Remove `none` from the allowed/signing algorithm list",
    );
    // The three facts the landing carries, and the one it must NOT: there is no safe overlap here, so
    // the sentence that says the sessions end has to stay, and no acceptance window may appear.
    for needle in [
        "the sessions END",
        "drives them to re-authenticate rather than into a retry loop",
        "still MINTS `none` in the same deploy",
        "Pin the verifier to the algorithm you actually issue",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/jwt-none-algorithm: the landing lost {needle:?}: {}",
            h[0].message
        );
    }
    assert!(
        h[0].message.contains("no overlapping window to plan"),
        "security/jwt-none-algorithm: the refusal of a dual-accept window must stay stated — without \
         it the next author borrows the rotation landing and tells the reader to keep honouring \
         `alg: none` for one token lifetime: {}",
        h[0].message
    );
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
        "import jwt from \"jsonwebtoken\";\nexport function verify(token: string) {\n  // zzop-jwt-none-algorithm-ok: local attack-simulation test harness, never runs against a real service\n  return jwt.verify(token, \"\", { algorithms: [\"none\"] });\n}\n",
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
        "import jwt from \"jsonwebtoken\";\nexport function verify(token: string) {\n  // zzop-jwt-verify-bypass-ok: dedicated expired-token regression test, not production code\n  return jwt.verify(token, \"secret\", { ignoreExpiration: true });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "jwt-verify-bypass").is_empty(),
        "{:?}",
        out.findings
    );
}

/// §27 pin (2026-08-28) for `jwt-no-expiry`, and the record of a sibling transplant that was DECLINED
/// on measurement (`1.architecture/rules/rule-quality.md` §27).
///
/// The remedy read: "Pass `{ expiresIn: '...' }` (or set `exp` in the payload) so tokens expire." The
/// defect is not that this breaks something — it is that it reaches NOTHING the finding is about, while
/// closing the finding. Measured on a hand-rolled HS256 signer/verifier (node v22.22.3), which is enough
/// to settle it because `exp` is a payload claim in the SIGNED bytes rather than a library behaviour:
///
///   * a token minted before the edit carries no `exp`, and its bytes are not touched by the edit;
///   * a verifier passing no `maxAge` still accepts it, after the edit and after the new tokens' own
///     lifetime has elapsed — measured `{ ok: true }` at both points;
///   * the two edits that DO retire it were measured to reject it: signing with a rotated secret
///     (signature mismatch) and requiring an `exp` claim on the verify side.
///
/// So a reader follows the prescription, watches the finding go green, and every unbounded token in
/// circulation is exactly as replayable as before. The corpus firing is that shape end to end: immich's
/// `server/src/repositories/crypto.repository.ts:63` wraps `jwt.sign`, and its one caller pairs it with a
/// `verifyJwt` that passes `{ algorithms: ['HS256'] }` and no `maxAge`.
///
/// ## Why the sibling's landing was NOT transplanted
///
/// `jwt-sign-literal-secret` writes this family's best paragraph on token invalidation, and
/// `rotation_landing.rs` guards it as a byte-identical shared constant across eight carriers. It was
/// read first and deliberately not copied here, because the two rules' VERBS differ and the landing is
/// about the verb: rotation replaces a live value, so it invalidates at the instant of the swap and its
/// landing is about migrating holders BEFORE that instant. Introducing an expiry is the opposite — it
/// reaches nothing that already exists, and splicing ROTATION_LANDING here would tell the reader their
/// edit ends live sessions when the measurement says it ends none. What carries over is the sibling's
/// last mile (a signing-key change invalidates every token already handed out, ending every live session
/// and 401-ing any sibling verifier) because that is precisely this rule's SECOND step — and it carries
/// over as a POINTER to that rule plus shared vocabulary, not as a ninth paraphrase of a constant whose
/// byte-identity test exists to stop exactly that.
///
/// Detection is untouched: the count and line asserted below are the pre-edit ones, and the two `absent`
/// negatives above still pin `expiresIn` and the `exp` claim.
#[test]
fn jwt_no_expiry_message_lands_the_backlog_the_remedy_cannot_reach() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issueToken() {\n  return jwt.sign(payload, \"secret\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "jwt-no-expiry");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
    let msg = &h[0].message;
    assert_disqualifier_summary_precedes_imperative(
        "jwt-no-expiry",
        msg,
        "ADDING AN EXPIRY DOES NOT REACH THE TOKENS THIS FINDING IS ABOUT",
        "pass `{ expiresIn: '...' }` (or set `exp` in the payload) so newly issued tokens expire",
        "a verifier that passes no `maxAge` has nothing to check it against",
    );
    for needle in [
        "rotating the signing secret, or making the verifier REQUIRE `exp`",
        "security/jwt-sign-literal-secret",
        "shared signing helper",
        "no refresh flow",
    ] {
        assert!(
            msg.contains(needle),
            "jwt-no-expiry: the landing lost {needle:?}: {msg}"
        );
    }
}

/// The transplant verdict above, made machine-checkable in the one direction that can rot: this message
/// must NOT acquire a copy of `ROTATION_LANDING`. A later editor reaching for the sibling's paragraph
/// would be copying a claim this rule's own measurement contradicts.
///
/// The needle is the landing's opening clause, which `rotation_landing.rs` pins as spelled exactly once
/// in each of the eight carriers.
#[test]
fn jwt_no_expiry_does_not_carry_the_rotation_landing() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issueToken() {\n  return jwt.sign(payload, \"secret\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "jwt-no-expiry");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert!(
        !h[0]
            .message
            .contains("BEFORE ROTATING, FIND WHAT IS STILL USING THIS VALUE"),
        "jwt-no-expiry: ROTATION_LANDING was spliced in. This rule's verb introduces an expiry, which \
         was MEASURED to invalidate nothing already issued; the rotation landing says the opposite and \
         belongs to the rules whose imperative replaces a live value. In: {}",
        h[0].message
    );
}
