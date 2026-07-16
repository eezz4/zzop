use crate::{hits, scan, TempDir};

// --- env-nonnull-assert ---

#[test]
fn process_env_non_null_assertion_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/config.ts",
        "export const key = process.env.API_KEY!;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "env-nonnull-assert");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn process_env_strict_inequality_comparison_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/config.ts",
        "export function checkEnv(): boolean {\n  if (process.env.API_KEY !== undefined) {\n    return true;\n  }\n  return false;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-nonnull-assert").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn env_assert_ok_marker_above_the_assertion_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/config.ts",
        "// env-assert-ok: validated at startup in bootstrap.ts\nexport const key = process.env.API_KEY!;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-nonnull-assert").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- debug-true-committed ---

#[test]
fn debug_true_flag_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/config.ts",
        "export const config = { debug: true, name: \"app\" };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "debug-true-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn tls_reject_unauthorized_disabled_via_env_assignment_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/bootstrap.ts",
        "process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "debug-true-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn reject_unauthorized_false_in_https_agent_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/client.ts",
        "export const agent = new (require(\"https\").Agent)({ rejectUnauthorized: false });\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "debug-true-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn debug_flag_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/config.ts",
        "// debug: true was removed here, see PR #123\nexport const config = { debug: false };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "debug-true-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn debug_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/config.ts",
        "export const config = { debug: true }; // debug-ok: local dev override, gated by NODE_ENV below\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "debug-true-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tls_reject_unauthorized_disabled_in_a_playwright_e2e_helper_is_not_flagged() {
    // `NODE_TLS_REJECT_UNAUTHORIZED=0` for a local self-signed cert in a Playwright e2e helper is the intended target, not a leaked dev backdoor — same exclusion as this pack's sibling `fullstack/localhost-egress-committed`.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "e2e/globalSetup.ts",
        "process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "debug-true-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tls_reject_unauthorized_disabled_in_a_test_file_by_suffix_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/setup.test.ts",
        "process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "debug-true-committed").is_empty(),
        "{:?}",
        out.findings
    );
}
