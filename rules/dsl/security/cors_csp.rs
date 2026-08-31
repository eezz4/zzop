use crate::{assert_disqualifier_clause_precedes_imperative, hits, label_of, scan, TempDir};

// --- cors-wildcard ---

#[test]
fn wildcard_access_control_allow_origin_header_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/middleware.ts",
        "declare const res: any;\nexport function setCors() {\n  res.setHeader(\"Access-Control-Allow-Origin\", \"*\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-wildcard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn wildcard_origin_config_property_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { origin: '*' };\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "cors-wildcard").len(), 1, "{:?}", out.findings);
}

#[test]
fn allowlisted_origin_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { origin: 'https://example.com' };\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cors-wildcard").is_empty(), "{:?}", out.findings);
}

/// POSITION, not presence (rule-quality.md §27). The clause that disqualifies this finding — a
/// public, credential-less API where `*` is the correct configuration — was already in the message,
/// 583 bytes BEHIND the imperative (measured 2026-08-26: clause at byte 808, imperative at 225 of
/// 986). A reader who acts on the first instruction never reaches it, and here the edit it leads to
/// is not a style change: cal.com's `apps/api/v2/src/bootstrap.ts:44` is the PUBLIC Platform API v2,
/// called from arbitrary customer domains by the `@calcom/atoms` React SDK it ships to those
/// customers. An allow-list cannot enumerate a multi-tenant public API's callers, so replacing the
/// wildcard fails every embedded integration at preflight.
///
/// This pin asserts ORDER. The `contains` checks stay green with the clause anywhere in the string,
/// which is exactly how it shipped behind the imperative. The invalidation probe: move the clause
/// back to the end of the message — every token below is still present and still spelled exactly
/// once, and this test must go red on order alone.
#[test]
fn the_public_api_exemption_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { origin: '*' };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-wildcard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;

    assert_disqualifier_clause_precedes_imperative(
        "cors-wildcard",
        m,
        "A wildcard is legitimate for a purely public, credential-less API",
        "Return a specific allow-listed origin instead",
    );
    // The exit the clause names has to survive the move, or the reader is told the finding may not
    // apply and given nothing to do about it.
    assert!(
        m.contains("zzop-cors-wildcard-ok"),
        "the clause no longer names the suppression marker: {m}"
    );
}

// --- cors-wildcard, Rust idiom arms (added 2026-08-03) ---

#[test]
fn tower_http_allow_origin_any_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/server.rs",
        "pub fn cors() -> CorsLayer {\n    CorsLayer::new().allow_origin(Any)\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-wildcard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
    assert_eq!(label_of(h[0]), Some("rust-tower-any"));
}

#[test]
fn tower_http_path_qualified_any_is_flagged_too() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/server.rs",
        "pub fn cors() -> CorsLayer {\n    CorsLayer::new().allow_origin(tower_http::cors::Any)\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "cors-wildcard").len(), 1, "{:?}", out.findings);
}

/// The nearest benign lookalike for the tower arm: same `.allow_origin(` call, but the argument is an
/// exact origin (`AllowOrigin::exact(...)`) — an UPPERCASE path-qualified identifier that is not `Any`.
/// The arm has to distinguish the argument, not just the method name.
#[test]
fn tower_http_allow_origin_exact_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/server.rs",
        "pub fn cors() -> CorsLayer {\n    CorsLayer::new().allow_origin(AllowOrigin::exact(\"https://app.example.com\".parse().unwrap()))\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cors-wildcard").is_empty(), "{:?}", out.findings);
}

#[test]
fn actix_allow_any_origin_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/server.rs",
        "pub fn cors() -> Cors {\n    Cors::default().allow_any_origin()\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-wildcard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("rust-actix-any"));
}

/// The nearest benign lookalike for the actix arm, twice over: `allowed_origin` (the specific-origin
/// method whose name embeds `allow`+`origin`) and `allow_any_method`/`allow_any_header` (the
/// `allow_any_*` family members that do NOT touch the origin).
#[test]
fn actix_allowed_origin_with_allow_any_method_and_header_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/server.rs",
        "pub fn cors() -> Cors {\n    Cors::default().allowed_origin(\"https://app.example.com\").allow_any_method().allow_any_header()\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cors-wildcard").is_empty(), "{:?}", out.findings);
}

#[test]
fn typed_header_constant_with_a_wildcard_value_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/server.rs",
        "pub fn headers() -> (HeaderName, HeaderValue) {\n    (header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static(\"*\"))\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-wildcard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("rust-header-const"));
}

/// The nearest benign lookalike for the typed-constant arm: the same constant beside an allow-listed
/// origin value. The `[^\"']*` bridge in the arm stops at the first quote, so the value it reads is the
/// one actually paired with the constant.
#[test]
fn typed_header_constant_with_an_allowlisted_value_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/server.rs",
        "pub fn headers() -> (HeaderName, HeaderValue) {\n    (header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static(\"https://app.example.com\"))\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cors-wildcard").is_empty(), "{:?}", out.findings);
}

// --- cors-credentials-wildcard ---

#[test]
fn credentials_true_alongside_a_wildcard_origin_in_the_same_file_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = {\n  origin: '*',\n  credentials: true,\n};\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-credentials-wildcard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn credentials_true_with_a_specific_origin_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = {\n  origin: 'https://example.com',\n  credentials: true,\n};\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "cors-credentials-wildcard").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- cors-reflected-origin-credentials ---

#[test]
fn credentials_true_then_origin_true_on_one_line_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { credentials: true, origin: true };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-reflected-origin-credentials");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn origin_true_then_credentials_true_on_one_line_is_flagged() {
    // Same co-occurrence, reversed key order — both orders must fire.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { origin: true, credentials: true };\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "cors-reflected-origin-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn origin_reflecting_request_headers_with_credentials_true_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "declare const req: any;\nexport const corsOptions = { origin: req.headers.origin, credentials: true };\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "cors-reflected-origin-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn credentials_true_with_a_specific_allowlisted_origin_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { credentials: true, origin: 'https://example.com' };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "cors-reflected-origin-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn multiline_cors_options_object_is_a_documented_limitation_and_not_flagged() {
    // Documented, deliberate limitation (not desired behavior): the matcher is single-line
    // co-occurrence, so splitting `origin`/`credentials` across separate lines evades it even
    // though the resulting configuration is exactly as vulnerable as the single-line shape above.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = {\n  origin: true,\n  credentials: true,\n};\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "cors-reflected-origin-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn cors_reflect_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "// zzop-cors-reflected-origin-credentials-ok: internal-only service mesh endpoint, never exposed publicly\nexport const corsOptions = { credentials: true, origin: true };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "cors-reflected-origin-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- csp-weak-or-disabled ---

#[test]
fn helmet_content_security_policy_false_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  contentSecurityPolicy: false,\n}));\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "csp-weak-or-disabled");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
    assert_eq!(label_of(h[0]), Some("helmet-csp-false"));
}

#[test]
fn csp_header_with_unsafe_inline_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const res: any;\nres.setHeader('Content-Security-Policy', \"default-src 'self' 'unsafe-inline'\");\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "csp-weak-or-disabled");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("csp-unsafe-inline"));
}

#[test]
fn csp_default_src_wildcard_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const res: any;\nres.setHeader('Content-Security-Policy', \"default-src *\");\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "csp-weak-or-disabled");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("csp-wildcard"));
}

#[test]
fn rust_header_value_with_unsafe_inline_is_flagged() {
    // Widened to `.rs` 2026-08-02 (U62): the unsafe-inline / default-src-wildcard arms are wire-header
    // VALUE shapes, so a Rust service spelling the literal header is judged like a JS one. Corpus twin:
    // cases/trees/rust-svc/src/config.rs.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/headers.rs",
        "pub fn csp() -> (&'static str, &'static str) {\n    (\"Content-Security-Policy\", \"default-src 'self'; script-src 'unsafe-inline'\")\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "csp-weak-or-disabled");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("csp-unsafe-inline"));
}

#[test]
fn rust_strict_csp_header_value_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/headers.rs",
        "pub fn csp() -> (&'static str, &'static str) {\n    (\"Content-Security-Policy\", \"default-src 'self'; script-src 'self'\")\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "csp-weak-or-disabled").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn helmet_content_security_policy_enabled_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  contentSecurityPolicy: true,\n}));\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "csp-weak-or-disabled").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn csp_wildcard_with_no_helmet_gate_present_is_not_flagged() {
    // require_file gate claim: the `csp-wildcard` label's own trigger text (`default-src *`)
    // does not itself contain "helmet"/"content-security-policy"/"contentSecurityPolicy", so it's
    // the one label where the gate is a real, non-tautological constraint — a file that never
    // mentions helmet or CSP anywhere stays silent even though the directive shape matches.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/flags.ts",
        "export const unrelatedConfig = {\n  defaultSrcNote: 'default-src * everywhere',\n};\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "csp-weak-or-disabled").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn csp_disabled_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "tests/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  contentSecurityPolicy: false,\n}));\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "csp-weak-or-disabled").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn csp_disabled_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  // zzop-csp-weak-or-disabled-ok: CSP is enforced at the CDN edge instead\n  contentSecurityPolicy: false,\n}));\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "csp-weak-or-disabled").is_empty(),
        "{:?}",
        out.findings
    );
}

/// §27 pin (2026-08-27). `csp-weak-or-disabled` shipped a remedy — "enable Helmet's default CSP ...,
/// drop `unsafe-inline` in favor of nonces/hashes" — with no statement of what happens between the
/// two halves of that sentence.
///
/// A CSP is enforced by the browser the moment the header ships, and it blocks before it reports.
/// Turning the default policy on, or removing `unsafe-inline`, stops every inline `<script>`/`<style>`,
/// every `on*=` handler and every third-party widget that injects one — and it does so at RUNTIME, in
/// the user's browser, so no build fails and no test goes red. "in favor of nonces/hashes" names the
/// destination and not the order, and the reader who ships the policy before the nonce wiring gets a
/// blank page with a green pipeline behind it.
///
/// The repair is the ORDER, and the order has a name the ecosystem already ships:
/// `Content-Security-Policy-Report-Only` collects the violations first, on real traffic, over the
/// routes a test suite never opens. The premise here is not answerable from the flagged line either —
/// "does this app render inline script?" is a question about templates the rule never reads, so §27's
/// third form (conditioning the imperative) would collapse to the bare imperative for exactly the
/// reader who needs the warning.
///
/// Detection untouched: same trigger, same label, same line, same `warning`.
#[test]
fn csp_message_lands_the_blocking_rollout_before_the_restrictive_policy_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  contentSecurityPolicy: false,\n}));\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "csp-weak-or-disabled");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
    assert_eq!(label_of(h[0]), Some("helmet-csp-false"));
    let msg = &h[0].message;
    let landing = msg
        .find("TIGHTENING CSP IS ENFORCED BY THE BROWSER")
        .expect("landing missing");
    let imperative = msg
        .find("Keep a restrictive policy:")
        .expect("imperative missing");
    assert_eq!(
        msg.matches("Keep a restrictive policy:").count(),
        1,
        "the imperative must be spelled ONCE or the offset comparison is meaningless: {msg}"
    );
    assert!(
        landing < imperative,
        "security/csp-weak-or-disabled: the imperative sits at byte {imperative}, AHEAD of the landing \
         at {landing} that says enforcement is immediate — a reader who ships the policy on the first \
         instruction takes the page dark and reads the rollout order afterwards: {msg}"
    );
    for needle in [
        "BLOCKS BEFORE IT REPORTS",
        "`Content-Security-Policy-Report-Only` FIRST",
        "a green pipeline is not evidence",
        "per-response nonce",
    ] {
        assert!(
            msg.contains(needle),
            "security/csp-weak-or-disabled: the landing lost {needle:?}: {msg}"
        );
    }
    // The report-only rollout has to be readable BEFORE the enforcing instruction, not after it.
    let report_only = msg
        .find("`Content-Security-Policy-Report-Only` FIRST")
        .expect("report-only missing");
    assert!(
        report_only < imperative,
        "security/csp-weak-or-disabled: the safe rollout order at {report_only} trails the imperative \
         at {imperative}: {msg}"
    );
}
