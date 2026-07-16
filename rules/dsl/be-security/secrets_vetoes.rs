use crate::{hits, scan, TempDir};

#[test]
fn vaguer_changeme_placeholder_without_a_recognized_mock_prefix_still_flagged() {
    // Not narrowed further: `changeme`-shaped placeholders have no recognized mock prefix and are
    // lexically indistinguishable from a real secret, so they intentionally stay flagged. A dash-joined
    // variant like `"changeme-please"` would instead match the letters-only, no-digits, dash-joined
    // sentinel shape (same family as `refresh-token`/`access-token`), so this fixture uses a dash-free
    // word-plus-digits value to test the actual decision under test: no entropy floor.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const apiKey = \"changeme12345\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn camel_case_mock_prefixed_token_is_not_flagged() {
    // `token: 'testAccessToken'` announces itself as a mock/placeholder by the "test" prefix, but a
    // mock-word veto whose right-hand boundary requires a delimiter/digit/quote/line-end immediately
    // after the mock word misses the camelCase continuation `A` (start of `AccessToken`) — the boundary
    // must also accept an uppercase letter right after the mock word.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/fixtures.ts",
        "export const accessOrWorkspaceAgnosticToken = { token: \"testAccessToken\", expiresAt: \"\" };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn lowercase_continuation_after_mock_word_does_not_over_broaden_the_veto() {
    // Guards against over-matching: the `(?-i:[A-Z])` boundary alternative is case-sensitive and only
    // accepts an uppercase letter right after the mock word, so a plain lowercase continuation like
    // "testimonial" must not gain the veto. This fixture is a real candidate (the `token` identifier
    // satisfies the `assignment` pattern, so the value does reach `exclude_pattern`) — "test" is
    // immediately followed by lowercase "i", which matches none of the boundary alternatives
    // (`[-_"'`]`, digit, `(?-i:[A-Z])`, or line-end), so the mock-word veto correctly does not engage
    // and the value stays flagged as a real secret.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const token = \"testimonial12345678\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn dash_delimited_sentinel_token_value_is_not_flagged() {
    // Dash-delimited multi-word lowercase tokens like `refresh-token`/`access-token`/`new-password`
    // are name/sentinel shapes identical in spirit to the excluded underscore-delimited ones
    // (`refresh_token = "refresh_token"`), just with dashes instead of underscores.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/fixtures.ts",
        concat!(
            "export const a = { refreshToken: { token: \"refresh-token\" } };\n",
            "export const b = { token: \"access-token\" };\n",
            "export const c = { password: \"new-password\" };\n",
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn dash_prefixed_random_looking_key_with_digits_is_still_flagged() {
    // Regression guard: the dash-sentinel veto only matches letters-only segments, so a genuinely
    // random-looking secret that happens to start with a recognized word + dash (digits and mixed case
    // breaking up the dash-joined run) must stay flagged, not get swept up by the veto meant for clean
    // dictionary-word placeholders like `sk-workspace-bound-secret`.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const secret = \"sk-a1B2c3D4e5F6g7H8i9J0\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn pascal_case_route_name_value_keyed_by_a_secret_shaped_identifier_is_not_flagged() {
    // A route-name registry can key route names by an enum member whose NAME carries a secret-shaped
    // suffix (`CHANGE_PASSWORD`, `FORGOT_PASSWORD`), but the VALUE is an unrelated PascalCase view
    // identifier, not a credential — same "value is a name/sentinel, not a secret" family as the
    // UPPER_SNAKE_CASE/lower_snake_case/dash-case shapes above, just PascalCase.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/constants/navigation.ts",
        concat!(
            "export enum VIEWS {\n",
            "  FORGOT_PASSWORD = 'ForgotMyPasswordView',\n",
            "  CHANGE_PASSWORD = 'ChangePasswordView',\n",
            "}\n"
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pascal_case_single_word_value_does_not_gain_the_multi_word_sentinel_veto() {
    // Regression guard: the PascalCase sentinel requires at least two capitalized segments (same
    // "multi-word" narrowness as the dash/underscore sentinel siblings) — a single PascalCase word is
    // not distinguishable from a real key that happens to be capitalized, so it must stay flagged.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        "export const apiKey = \"Changemeplease\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}
