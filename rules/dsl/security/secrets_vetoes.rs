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

#[test]
fn a_ui_class_name_value_keyed_by_a_secret_shaped_property_is_not_flagged() {
    // Calibration pin (7/7 corpus FPs before the fix): the rule matched on the PROPERTY NAME alone,
    // with no value-side check at all — `{ token: "mantine-DatePickerInput-input" }` in a lint script
    // is a CSS class, not a credential. The generalized identifier-shape veto (letter-only words joined
    // by `-`/`_`, ANY casing) is what rejects it; the old veto only covered all-lowercase kebab.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/lint/classNames.ts",
        "export const selectors = { token: \"mantine-DatePickerInput-input\" };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_mixed_case_underscore_joined_identifier_value_is_not_flagged() {
    // Same shape via `_` instead of `-`, and in a casing the three old casing-specific vetoes missed.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/lint/keys.ts",
        "export const cfg = { apiKey: \"Mantine_DatePicker_Input\" };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_dash_joined_value_whose_segment_carries_digits_still_fires() {
    // Positive pin for the value-shape gate's boundary: the veto is letter-ONLY segments, so a
    // credential-shaped value keeps firing even though it contains a dash. Guards the same property
    // the pre-existing `dash_prefixed_random_looking_key_with_digits_is_still_flagged` pin does, from
    // the other side — this one would break if the veto were loosened to allow digits in a segment.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/creds.ts",
        "export const secret = \"live-a1B2c3D4e5F6g7H8\";\n",
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
fn the_seventh_measured_finding_is_a_separator_free_word_and_deliberately_still_fires() {
    // Honesty pin for the calibration claim. Re-running the matcher regex over the whole corpus that
    // produced the reported 7/7 shows all nine candidate lines live in ONE file — a lint script's
    // `ALLOW` table of `{ token, reason }` rows. Two of the nine were already vetoed before the
    // value-shape gate (one all-lowercase-kebab value, and one whose `reason` prose incidentally
    // contains a quoted `'image-translate'` that trips the whole-line veto), which is exactly why the
    // report said 7 and not 9. The value-shape gate drops six more; this one is the leftover.
    // `"adsbygoogle"` is a separator-free single word, so it has no segments for the identifier-shape
    // veto to see — and it is lexically indistinguishable from the weak literal password in the second
    // line below, which MUST keep firing. Widening the veto to separator-free lowercase words is
    // therefore refused: it would buy one corpus finding by shipping a false negative on every
    // all-lowercase hardcoded password.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "scripts/lint/lint-e2e-selectors.cjs",
        concat!(
            "const ALLOW = [\n",
            "  { token: \"adsbygoogle\", reason: \"Google AdSense runtime-injected element\" },\n",
            "];\n",
            "const conn = { password: \"letmeinplease\" };\n",
        ),
    );
    let out = scan(&dir);
    let mut lines: Vec<u32> = hits(&out, "hardcoded-secret")
        .iter()
        .map(|f| f.line)
        .collect();
    lines.sort_unstable();
    assert_eq!(lines, vec![2, 4], "{:?}", out.findings);
}

#[test]
fn a_passphrase_shaped_credential_is_silenced_by_the_value_shape_veto() {
    // SEAL TEST for the gate's measured blind spot — this asserts a FALSE NEGATIVE on purpose, so the
    // limit cannot be forgotten and cannot change silently. A credential built from dictionary words
    // joined by `-`/`_` is byte-for-byte the same shape as the CSS class names the veto exists to
    // reject, so it is always silenced. Simulation over 200k random values per length puts the
    // collateral loss on random base64url tokens at ~0.9% (24 chars) / ~0.26% (32) / ~0.04% (43) — the
    // ones that happen to draw no digit — and at 0% for base64-standard, hex and alphanumeric values,
    // whose alphabets contain no `-`/`_` at all. If a future change makes any line below fire, that is
    // an IMPROVEMENT, not a regression: re-measure, then update this test and the rule message
    // together. The rule message states the same limit to the reader.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/config.ts",
        concat!(
            "export const apiKey = \"correct-horse-battery-staple\";\n",
            "export const secret = \"trombone_ravine_wallet_ember\";\n",
            "export const token = \"kQxvNbHeLmRaZtYw-PcSdUfGjHkLn\";\n",
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "documented blind spot changed — re-measure before editing: {:?}",
        out.findings
    );
}

#[test]
fn the_common_vendor_key_prefixes_all_survive_the_value_shape_veto() {
    // Counterweight to the seal test above: the shapes that actually leak in the wild are unaffected.
    // Each value here is a synthetic placeholder in a real vendor's format, not a live credential.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/creds.ts",
        concat!(
            "export const a = { apiKey: \"AKIAABCDEFGHIJKLMNOP\" };\n",
            "export const b = { secret: \"sk_live_51H8xQ2Lm3nP4rS5tU6vW\" };\n",
            "export const c = { token: \"ghp_16C7e42F292c6912E7710c838347Ae178B4a\" };\n",
            // Split literal, same convention (and same reason) as `vendor_token_committed.rs`'s
            // header: GitHub push protection scans RAW SOURCE for a well-formed Slack token and does
            // not care that the body is obviously synthetic. It blocked a release push over this exact
            // line. `concat!` reassembles it at compile time, so the written fixture is unchanged.
            "export const d = { token: \"xo",
            "xb-2345678901-2345678901234-AbCdEfGhIjKlMnOpQrStUvWx\" };\n",
            "export const e = { secret: \"nRvyYC4soFxBdZ-F-5Nnzz5USXstR1YylsTd-mA0aKtI\" };\n",
        ),
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        5,
        "{:?}",
        out.findings
    );
}
