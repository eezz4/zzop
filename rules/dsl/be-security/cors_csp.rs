use crate::{hits, label_of, scan, TempDir};

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
        "// cors-reflect-ok: internal-only service mesh endpoint, never exposed publicly\nexport const corsOptions = { credentials: true, origin: true };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "cors-reflected-origin-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- csp-disabled ---

#[test]
fn helmet_content_security_policy_false_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  contentSecurityPolicy: false,\n}));\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "csp-disabled");
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
    let h = hits(&out, "csp-disabled");
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
    let h = hits(&out, "csp-disabled");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("csp-wildcard"));
}

#[test]
fn helmet_content_security_policy_enabled_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  contentSecurityPolicy: true,\n}));\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "csp-disabled").is_empty(), "{:?}", out.findings);
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
    assert!(hits(&out, "csp-disabled").is_empty(), "{:?}", out.findings);
}

#[test]
fn csp_disabled_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "tests/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  contentSecurityPolicy: false,\n}));\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "csp-disabled").is_empty(), "{:?}", out.findings);
}

#[test]
fn csp_disabled_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  // csp-disabled-ok: CSP is enforced at the CDN edge instead\n  contentSecurityPolicy: false,\n}));\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "csp-disabled").is_empty(), "{:?}", out.findings);
}
