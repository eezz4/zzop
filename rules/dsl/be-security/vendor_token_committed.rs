use crate::{hits, scan, TempDir};

// --- vendor-token-committed ---
//
// These fixtures must carry live vendor-token SHAPES by design (that is what the rule detects),
// but GitHub push-protection scans raw source text for those same shapes. So each Stripe token is
// assembled from split literals via `concat!` — no contiguous `sk_live_`/`rk_live_`/`sk_test_`
// string ever appears in this file, while the rule still analyzes the full reassembled token in the
// synthetic file content. Bodies are obviously-synthetic (not real keys).
const STRIPE_LIVE: &str = concat!("sk_li", "ve_FAKEexampleonly0notarealkey01");
const STRIPE_RK: &str = concat!("rk_li", "ve_FAKEexampleonly0notarealkey01");
const STRIPE_TEST: &str = concat!("sk_te", "st_FAKEexampleonly0notarealkey01");
const GH_PAT: &str = concat!("gh", "p_abcdefghij1234567890ABCDEFGHIJ123456");
const GH_OAUTH: &str = concat!("gh", "o_abcdefghij1234567890ABCDEFGHIJ123456");
const SLACK_BOT: &str = concat!("xo", "xb-1234567890-abcdEFGH1234");
const GOOGLE_API: &str = concat!("AI", "zaabcdefghij1234567890ABCDEFGHIJ12345");

#[test]
fn stripe_live_secret_key_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/billing.ts",
        &format!("export const stripeKey = \"{STRIPE_LIVE}\";\n"),
    );
    let out = scan(&dir);
    let h = hits(&out, "vendor-token-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn stripe_live_restricted_key_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/billing.ts",
        &format!("export const stripeRestrictedKey = \"{STRIPE_RK}\";\n"),
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "vendor-token-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn stripe_test_secret_key_is_deliberately_not_flagged() {
    // Documented claim: sk_test_ is a test-mode key, not a production credential, and is
    // deliberately NOT matched.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/billing.ts",
        &format!("export const stripeKey = \"{STRIPE_TEST}\";\n"),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "vendor-token-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn github_personal_access_token_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "scripts/deploy.ts",
        &format!("export const ghToken = \"{GH_PAT}\";\n"),
    );
    let out = scan(&dir);
    let h = hits(&out, "vendor-token-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn github_oauth_token_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "scripts/deploy.ts",
        &format!("export const ghToken = \"{GH_OAUTH}\";\n"),
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "vendor-token-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn slack_bot_token_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "integrations/slack.ts",
        &format!("export const slackToken = \"{SLACK_BOT}\";\n"),
    );
    let out = scan(&dir);
    let h = hits(&out, "vendor-token-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn google_api_key_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "config/maps.ts",
        &format!("export const mapsKey = \"{GOOGLE_API}\";\n"),
    );
    let out = scan(&dir);
    let h = hits(&out, "vendor-token-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn stripe_live_key_read_from_process_env_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/billing.ts",
        &format!("export const stripeKey = process.env.STRIPE_KEY || \"{STRIPE_LIVE}\";\n"),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "vendor-token-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn vendor_token_committed_in_a_test_fixture_path_is_still_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/__tests__/billing.test.ts",
        &format!("export const stripeKey = \"{STRIPE_LIVE}\";\n"),
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "vendor-token-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn vendor_token_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/billing.ts",
        &format!("// vendor-token-ok: rotated dummy value kept only for a format-parsing regression test\nexport const stripeKey = \"{STRIPE_LIVE}\";\n"),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "vendor-token-committed").is_empty(),
        "{:?}",
        out.findings
    );
}
