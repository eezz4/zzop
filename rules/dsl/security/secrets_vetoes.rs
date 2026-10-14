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
    //
    // EVERY vendor token below is split across two literals, the convention `vendor_token_committed.rs`'s
    // header established: GitHub push protection scans RAW SOURCE for well-formed tokens and does not
    // care that a body is obviously synthetic or that a comment says so. It blocked the v0.24.0 release
    // push over the Slack line here. `concat!` reassembles each at compile time, so the file content this
    // test writes -- and the five findings asserted below -- are unchanged. Only the Slack line was split
    // at the time of that incident; the other three merely happened not to be flagged that run, so they
    // are split too rather than left as the next release's blocker (scripts/check-vendor-token-literals.sh
    // now enforces this mechanically).
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/creds.ts",
        concat!(
            "export const a = { apiKey: \"AK",
            "IAABCDEFGHIJKLMNOP\" };\n",
            "export const b = { secret: \"sk_li",
            "ve_51H8xQ2Lm3nP4rS5tU6vW\" };\n",
            "export const c = { token: \"gh",
            "p_16C7e42F292c6912E7710c838347Ae178B4a\" };\n",
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

/// **A shell/template placeholder is a placeholder whichever way it is spelled.** The SCREAMING_SNAKE
/// veto required `[A-Z]` immediately after the opening quote, so the bare `"LLM_API_KEY"` was excluded
/// while `"$LLM_API_KEY"` and `"${LLM_API_KEY}"` — the two spellings an OpenAPI example or a `.env`
/// template actually uses — were reported. Measured on meilisearch `577f7af`: 3 of its 22 findings were
/// one such line repeated across three `#[utoipa::path]` examples in a single file, each carrying a
/// full credential-rotation message.
///
/// All three placeholder spellings ride with a REAL secret in the same call, because a veto is only
/// worth having if something still fires beside it.
#[test]
fn a_dollar_or_braced_screaming_snake_placeholder_is_vetoed_like_the_bare_one() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "src/openapi.ts",
        "export const examples = {\n\
        \x20 bare: { apiKey: \"LLM_API_KEY\" },\n\
        \x20 shell: { apiKey: \"$LLM_API_KEY\" },\n\
        \x20 braced: { apiKey: \"${LLM_API_KEY}\" },\n\
        };\n",
    );
    // Split so no contiguous vendor-token prefix+body survives in tracked source — the value the scan
    // sees is unchanged. Established convention: `vendor_token_committed.rs`'s header.
    dir.write(
        "src/leak.ts",
        &format!(
            "export const apiKey = \"{}{}\";\n",
            "sk_", "live_9f3ba21c8e7d4a6b5c0f1e2d3a4b5c6d"
        ),
    );
    let out = scan(&dir);
    let files: Vec<&str> = hits(&out, "hardcoded-secret")
        .iter()
        .map(|f| f.file.as_str())
        .collect();
    assert_eq!(
        files,
        vec!["src/leak.ts"],
        "all three placeholder spellings must be vetoed and the real credential must fire: {:?}",
        out.findings
    );
}

/// 🔴 The shape that was 7 of 9 rows on one outside audit's screen, and 0 of the 9 was a secret.
///
/// `config-file-secret`'s veto list already covered every OTHER placeholder spelling —
/// `${VAR}`, `$VAR`, `<angle>`, and the mock-word family — so a reader could reasonably read its
/// silence on `{braces}` as a judgement. It was an omission. Measured on a 9-line fixture before this
/// landed: `databricks://token:{access_token}@{host}:{port}` fired while the real 20-character secret
/// three lines below it fired too, so the screen gave the two the same weight.
///
/// The value here is a DOCUMENTED CONNECTION TEMPLATE — the thing a database-catalog file is for — and
/// rotating it is not merely useless but impossible: there is nothing behind the braces.
#[test]
fn a_brace_placeholder_connection_template_is_not_a_committed_secret() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "databases.yml",
        "dsn: \"databricks://token:{access_token}@{host}:{port}\"\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "config-file-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The boundary that makes the veto safe, and the reason it is anchored to the VALUE position rather
/// than to "the line contains braces". A TOML inline table opens with `{` immediately followed by an
/// identifier — byte-identical to a placeholder's opening — so a veto written as "a brace appears
/// anywhere" would silence every real secret inside one.
#[test]
fn a_real_secret_inside_a_toml_inline_table_keeps_firing() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app.toml",
        "creds = {password = \"Hx7Qd9Zt2Lm4Pk8Vb3Nc\"}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "config-file-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

/// The other half of that boundary: a real secret nested one level inside a JSON object. The enclosing
/// `{` is followed by whitespace rather than an identifier, which is what keeps it out of the veto —
/// and this is the case that sat three lines below the placeholder in the fixture that found the gap.
#[test]
fn a_real_secret_nested_in_a_json_object_keeps_firing() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "conf.json",
        "{\n  \"real\": { \"password\": \"Hx7Qd9Zt2Lm4Pk8Vb3Nc\" }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "config-file-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

/// A real secret whose TRAILING COMMENT mentions a brace token. The veto reads the value position, so a
/// placeholder-looking word after the closing quote cannot reach it — without this anchor the rule would
/// lose any secret documented with a `{...}` reference beside it.
#[test]
fn a_brace_token_in_a_trailing_comment_does_not_veto_the_secret() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "svc.properties",
        "password = Hx7Qd9Zt2Lm4Pk8Vb3Nc  # rotate per {runbook}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "config-file-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

/// 🔴 The whole yield of this rule on one 3,363-file tree, and it was wrong — precision 0/1.
///
/// `server-password: CI_DEPLOY_PASSWORD` inside an `actions/setup-java` block is the DOCUMENTED
/// contract of that key: it takes the NAME of the environment variable holding the credential, and the
/// real value arrives eight lines below as `CI_DEPLOY_PASSWORD: ${{ secrets.CI_DEPLOY_PASSWORD }}`.
/// The matched token is 19 characters, so it clears this rule's own 16-character floor, and it is
/// byte-identical to the semantic name of its own key.
///
/// Why the veto keys on the SECRET WORD being a segment rather than on "the value looks like an
/// environment variable": Rust's regex engine has no backreferences, so "the value equals the key" is
/// not expressible here. An all-caps-with-underscores shape alone is too wide — it would silence a
/// human-chosen passphrase like `CORRECT_HORSE_BATTERY_STAPLE`, which is pinned below.
#[test]
fn a_value_that_is_the_env_var_name_for_the_secret_is_not_the_secret() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("ci.yml", "      server-password: CI_DEPLOY_PASSWORD\n");
    let out = scan(&dir);
    assert!(
        hits(&out, "config-file-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The upper boundary: an all-caps, underscore-joined value that is NOT naming a credential variable is
/// a passphrase, and this rule must keep it. This is the case that rules out the wider veto.
#[test]
fn an_all_caps_passphrase_is_still_a_committed_secret() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "svc.properties",
        "password = CORRECT_HORSE_BATTERY_STAPLE\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "config-file-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

/// 🔴 The regression this rule's own `(?i)` prefix invites, and the reason the new alternative is
/// wrapped in `(?-i:)`.
///
/// `exclude_pattern` is case-INSENSITIVE for the whole expression, so an alternative written as
/// `[A-Z][A-Z0-9]*_...` matches lowercase too — and then a real secret spelled
/// `my_password_value_here` is silenced by a veto whose entire justification was that the value is
/// SHOUTED like a variable name. The uppercase requirement is the discriminator; without the scoped
/// flag it does not exist.
#[test]
fn a_lowercase_underscore_secret_is_not_caught_by_the_env_var_name_veto() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("app.conf", "password = my_password_value_here\n");
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "config-file-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

/// The other direction of the same discriminator: a real secret that merely CONTAINS the keyword. The
/// veto requires every segment after the keyword to be all-caps, so a mixed-case tail keeps the finding.
#[test]
fn a_secret_whose_text_contains_the_keyword_keeps_firing() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("app.ini", "password = MY_PASSWORD_IS_Hx7Qd9Zt2Lm\n");
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "config-file-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}

/// The two rows that survived the brace veto on one 10,585-file tree, and both were still false: a
/// generated OpenAPI spec writes its sample request body under `"example":` (SINGULAR), while this
/// rule's enclosing-call veto named only `"examples"`. One character.
///
/// Measured on superset `e7dccd44a7`: this rule's full yield was 24 rows and not one was a secret —
/// 22 brace-placeholder connection templates in `databases.json` plus these 2. After both vetoes it
/// is 0, and on mall `0504e86b1f` the count is unchanged at 2, so the narrowing cost nothing there.
#[test]
fn a_sample_value_under_a_singular_example_key_is_not_a_committed_secret() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "openapi.json",
        "{\n  \"example\": {\n    \"password\": \"complex-password-here\"\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "config-file-secret").is_empty(),
        "{:?}",
        out.findings
    );
}
