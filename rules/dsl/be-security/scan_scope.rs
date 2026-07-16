use crate::{hits, scan, TempDir};

// --- skip_comment_lines + test-path file_exclude_pattern ---
// Without `skip_comment_lines`, a commented-out example of a matched shape (e.g. the `mass-assignment`
// body-passthrough shape) would fire on `method-scan` rules. Deployed-surface rules in this pack
// (everything except `hardcoded-secret`/`hardcoded-password`) exclude test-path files via the
// shared `file_exclude_pattern`.

#[test]
fn mass_assignment_shape_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/users.ts",
        "declare const prisma: any;\nexport async function updateUser(req: any) {\n  // prisma.user.update({ data: req.body }) -- old unsafe version, replaced below\n  return null;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mass-assignment").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn cookie_set_without_httponly_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/__tests__/auth.test.ts",
        "declare const res: any;\ndeclare const token: string;\nexport function login() {\n  res.cookie(\"session\", token);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "insecure-cookie").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn hardcoded_secret_in_a_test_fixture_path_is_still_flagged() {
    // `hardcoded-secret` (and `hardcoded-password`) are repo-content rules, not deployed-surface,
    // so unlike the rest of this pack they don't exclude test-fixture paths — a real secret committed
    // inside a test file is still a leaked credential the moment it's pushed.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/__tests__/config.test.ts",
        "export const apiKey = \"abcd1234efgh5678\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}
