//! §27 POSITION PINS for the eight rules that tell a reader to rotate or revoke a credential —
//! `hardcoded-secret`, `high-entropy-secret`, `config-file-secret`, `private-key-committed`,
//! `vendor-token-committed`, `conn-string-credentials`, `hardcoded-password`,
//! `jwt-sign-literal-secret`.
//!
//! The EIGHTH arrived on 2026-08-27 and is the reason this test exists in the shape it does.
//! `jwt-sign-literal-secret` ends in `rotate the exposed key` — the same verb, so the same landing,
//! and the byte-identity test below is what forced that to be a decision rather than a second
//! paraphrase. It is also the one member with a SECOND breakage the landing does not describe: the
//! other seven strand CONSUMERS holding the value, and a signing key additionally strands every token
//! it already signed, which is not a holder anyone can migrate. That half is jwt-specific, so it lives
//! in that rule's exit — the same place every other per-rule divergence lives — and says where the
//! overlapping window actually sits for a signing key: on the VERIFY side, accepting both keys for the
//! length of the longest token lifetime. The sibling landing for "the artifact outlives the change"
//! (`algorithm_cutover_landing.rs`) is deliberately NOT also spliced here, because the two name
//! different failures and this rule's is the first one.
//!
//! CORRECTED 2026-08-31. This comment used to say "two 700-character landings in one message is the
//! §30 failure that batch measured and trimmed (538 -> 346)", and a later batch scoped itself by that
//! sentence -- it deferred an owed landing believing it would breach a measured cap. §30 is cited
//! correctly (the 538 -> 346 measurement is there) but summarised wrongly. What §30 measured is that
//! introducing a class with no prior premise costs ten times §27's ~50-character conditioning band,
//! and it trimmed the result because the gate had already removed the population that would read the
//! trimmed part. That is a REACH argument, not a length limit, and no per-message landing length cap
//! exists anywhere in this tree -- §37's own landings run 1,871 and 2,069 characters.
//!
//! The prescription-breaking veto these close (`1.architecture/rules/rule-quality.md` §27):
//! rotating a LIVE credential invalidates the old value at the instant the replacement is issued,
//! so every deploy, CI job, cron, inbound webhook, sibling service and third-party console still
//! holding it starts failing authentication — and that outage is what gets noticed, before the leak
//! the reader was fixing. Measured over the seven messages on 2026-08-26, `downtime`, `outage`,
//! `coordinat`, `still using` and `401` occurred ZERO times in all seven (canary: `credential`
//! occurred 12/5/8/0/5/7/2 over the same seven, so the counter was reading the strings).
//!
//! One constant for all seven, byte-identical. The dangerous property belongs to rotation itself,
//! not to what any one rule detects, and the rules diverge only in their EXIT — which is each
//! rule's own imperative. Same split `DATA_LOSS_LANDING` uses in `rules/native/rules-schema`
//! (`cd891eb`), and the same reason: a position pin needs ONE spelling to index.
//!
//! POSITION, not presence. A reader who acts on the first instruction never reaches a caveat placed
//! behind it, so `contains` proves nothing. The invalidation probe for every test below is to move
//! `ROTATION_LANDING` to the tail of the message: every token stays present and spelled exactly
//! once, and each of these seven must go red on ORDER alone.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

/// The rotation landing, spliced ahead of the rotate/revoke imperative in all seven messages. Not a
/// `convention` value — no project declares it; it is zzop's own sentence about what the reader's
/// own edit does to a running system.
const ROTATION_LANDING: &str = "BEFORE ROTATING, FIND WHAT IS STILL USING THIS VALUE — deploys, CI jobs, cron, inbound webhooks, sibling services, and any third-party console it was pasted into. Rotation is not additive: issuing the replacement invalidates the old value at that instant, so every holder you did not migrate starts failing auth (401) as you finish, and that outage is noticed before the leak is. Where the issuer will hold two live values, use an OVERLAPPING WINDOW — mint the new one, move each consumer, revoke the old; where it holds only one, swap in a coordinated window with those owners on hand. Urgency and coordination are not in tension: a leaked value is compromised NOW, and a cutover is arranged in minutes, not releases.";

#[test]
fn hardcoded_secret_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const apiKey = \"Pr0d!Postgres#2024\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "hardcoded-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "hardcoded-secret",
        &h[0].message,
        ROTATION_LANDING,
        "rotate/revoke the actual credential too",
    );
}

#[test]
fn high_entropy_secret_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const dbPassword = 'correct-horse-battery-staple';\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "high-entropy-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "high-entropy-secret",
        &h[0].message,
        ROTATION_LANDING,
        "Rotate/revoke the actual credential, then move the replacement",
    );
}

#[test]
fn config_file_secret_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/resources/application.properties",
        "jwt.secret=nRvyYC4soFxBdZ-F-5Nnzz5USXstR1YylsTd-mA0aKtI\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "config-file-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "config-file-secret",
        &h[0].message,
        ROTATION_LANDING,
        "Then rotate/revoke the committed value",
    );
}

#[test]
fn private_key_committed_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "secrets/id_rsa.pem",
        "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQ...\n-----END PRIVATE KEY-----\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "private-key-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "private-key-committed",
        &h[0].message,
        ROTATION_LANDING,
        "Start the rotation immediately",
    );
}

#[test]
fn vendor_token_committed_landing_precedes_the_imperative() {
    // Split literal, same convention as `vendor_token_committed.rs` — GitHub push protection scans
    // raw source for a well-formed key and does not read the comment saying the body is synthetic.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/billing.ts",
        concat!(
            "export const stripeKey = \"sk_li",
            "ve_FAKEexampleonly0notarealkey01\";\n"
        ),
    );
    let out = scan(&dir);
    let h = hits(&out, "vendor-token-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "vendor-token-committed",
        &h[0].message,
        ROTATION_LANDING,
        "Rotate the credential through the vendor's own key-rotation/revocation flow",
    );
}

#[test]
fn conn_string_credentials_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = \"postgres://user:hunter2@host:5432/db\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "conn-string-credentials");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "conn-string-credentials",
        &h[0].message,
        ROTATION_LANDING,
        "Then rotate the credential (treat it as already compromised)",
    );
}

#[test]
fn hardcoded_password_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Config.java",
        "public class Config {\n    public String password = \"sup3rSecretPwd\";\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "hardcoded-password");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "hardcoded-password",
        &h[0].message,
        ROTATION_LANDING,
        "rotate/revoke the actual credential too",
    );
}

/// `jwt-sign-literal-secret` — the eighth carrier (2026-08-27). Its rotate imperative is spelled the
/// same way the seven are, so it takes the same landing rather than a paraphrase; what it adds is the
/// verify-side window, pinned below because it is the half the shared sentence cannot state.
#[test]
fn jwt_sign_literal_secret_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const jwt: any;\ndeclare const payload: any;\nexport function issue() {\n  return jwt.sign(payload, \"abcd1234efgh5678\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "jwt-sign-literal-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "jwt-sign-literal-secret",
        &h[0].message,
        ROTATION_LANDING,
        "rotate the exposed key",
    );
    for needle in [
        "THAT WINDOW IS ON THE VERIFY SIDE",
        "stops verifying the instant you sign with a new one",
        "verifying against BOTH keys",
        "longest token lifetime",
        "a forced re-login is acceptable",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/jwt-sign-literal-secret: the verify-side exit lost {needle:?}: {}",
            h[0].message
        );
    }
}

/// The landing is one constant for eight rules, so the thing that can silently rot is a COPY that
/// drifts. This asserts the shipped pack carries it byte-identically in all eight and NOWHERE else —
/// a rule that acquires a rotate imperative later and paraphrases the landing fails here rather than
/// shipping a second spelling that the position pins above would each still pass.
#[test]
fn the_landing_is_byte_identical_in_exactly_the_eight_rotation_rules() {
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("security.json")).expect("security.json parses");
    let mut carriers: Vec<&str> = pack["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .filter(|r| {
            r["message"]
                .as_str()
                .is_some_and(|m| m.contains(ROTATION_LANDING))
        })
        .map(|r| r["id"].as_str().expect("rule id"))
        .collect();
    carriers.sort_unstable();
    assert_eq!(
        carriers,
        [
            "config-file-secret",
            "conn-string-credentials",
            "hardcoded-password",
            "hardcoded-secret",
            "high-entropy-secret",
            "jwt-sign-literal-secret",
            "private-key-committed",
            "vendor-token-committed",
        ],
        "the rotation landing is carried by a different rule set than the seven it was measured for"
    );
}
