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
//! ONE constant for every carrier, byte-identical — the count lives in the pin below
//! (`the_landing_is_byte_identical_in_exactly_the_eight_rotation_rules`), which derives the carrier
//! set from the shipped pack rather than restating it. 🔴 This line said "all seven" until
//! 2026-10-09, a year-and-a-half-stale cardinal: the eighth carrier arrived 2026-08-27 and this
//! file records its arrival 148 lines down. The dangerous property belongs to rotation itself,
//! not to what any one rule detects, and the rules diverge only in their EXIT — which is each
//! rule's own imperative. Same split `DATA_LOSS_LANDING` uses in `rules/native/rules-schema`
//! (`cd891eb`), and the same reason: a position pin needs ONE spelling to index.
//!
//! POSITION, not presence. A reader who acts on the first instruction never reaches a caveat placed
//! behind it, so `contains` proves nothing. The invalidation probe for every test below is to move
//! `ROTATION_LANDING` to the tail of the message: every token stays present and spelled exactly
//! once, and each carrier must go red on ORDER alone.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

/// The rotation landing, spliced ahead of the rotate/revoke imperative in every carrier's message. Not a
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

    // THE COMPLETENESS HALF, and the reason this rule's `caveat` is the only one of the eight that is
    // not just the shared landing (outside audit round 13, measured on macrozheng/mall `0504e86`).
    //
    // The reply folds this rule to `messageBy: "ruleId"`, so a reader holding a fired finding gets the
    // `caveat` and nothing else. The caveat was the rotation runbook alone, which answers "how do I
    // change this value" — and leaves out the one fact that changes what the reader DOES in the file
    // they were just sent to. Measured on that tree: 25 credential-shaped lines across 8 YAML files,
    // 2 reported. The mechanism is one number, confirmed by construction with a single factor varied
    // (`password: 123456` silent, `password: 1234567890123456` fires): the value must clear 16
    // characters. The message already said so, naming three of that tree's own lines and adding
    // "including when they sit in a production Spring profile" — and the fold deleted exactly that
    // sentence while keeping the runbook. So the sentence moved to sit directly behind the landing and
    // the caveat span now covers both.
    //
    // Why this is NOT the false-negative disclosure `rejected.md` D93 refuses: D93's subject is a shape
    // somewhere ELSE in the tree, which says nothing to someone reading a finding that fired. This one's
    // subject is THE FILE THE READER IS BEING SENT TO, and it changes what they do there — they grep it
    // themselves. The rule's own prose already drew that line: "stated because this rule's silence is
    // easy to read as clean".
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("security.json")).expect("security.json parses");
    let caveat = pack["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .find(|r| r["id"].as_str() == Some("config-file-secret"))
        .and_then(|r| r["caveat"].as_str())
        .expect("config-file-secret carries a caveat")
        .to_string();
    assert!(
        caveat.starts_with(ROTATION_LANDING),
        "the shared landing must stay the caveat's PREFIX — the rotation hazard is still the first \
         thing a reader of a true positive needs: {caveat}"
    );
    assert!(
        caveat.contains("the value must be 16+ characters"),
        "the caveat must carry the value floor, because on a folded row it is the only channel the \
         reader has and the floor is what makes this rule's silence unsafe to read as clean: {caveat}"
    );
    assert!(
        caveat.contains("production Spring profile"),
        "and must keep the measured instance, so the floor is a fact rather than a hedge: {caveat}"
    );
    // ORDER inside the message: landing, then the floor, then the imperative. A floor printed after
    // the imperative is a floor the reader never reaches.
    let m = &h[0].message;
    let i_landing = m.find(ROTATION_LANDING).expect("landing present");
    let i_floor = m.find("WHAT IT STILL CANNOT SEE").expect("floor present");
    let i_imperative = m
        .find("Then rotate/revoke the committed value")
        .expect("imperative present");
    assert!(
        i_landing < i_floor && i_floor < i_imperative,
        "order must be landing -> floor -> imperative, got {i_landing}/{i_floor}/{i_imperative}"
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
        "the rotation landing is carried by a different rule set than this pin was written for. A \
         rule that GAINED the landing is the good direction and the array is what to update; a rule \
         that LOST it means a reader is being told to rotate with no landing ahead of the imperative"
    );
}
