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

// --- the extensions a parser DOES claim: `.jsx`/`.mts`/`.cts` parity with `.ts` ---
//
// The canary that found the hole, kept as the pin that closes it. A React tree is `.jsx`, the
// TypeScript frontend claims `.jsx` (`dispatch_by_extension` maps it, and `parse.rs` enables JSX
// syntax for it), and until 2026-08-21 every parser-backed security rule's `file_pattern` stopped at
// `ts|tsx|js|mjs|cjs` — so on a React tree the entropy channel was silent and that silence read as
// "inventory clean". Byte-identical literals, two extensions, one assertion.
//
// SCOPE, because this pin covers less than the change that produced it. Ten parser-backed rules were
// widened; this asserts BEHAVIOUR for the two a secret literal reaches, and reverting any of the other
// eight leaves it green. What holds those is `dsl_inline_value_census`, which pins all 51 of this
// pack's `file_pattern` values by exact text: a revert is a snapshot mismatch there, with the row's own
// rationale comment printed beside it. The split is deliberate rather than a shortfall left unfinished
// -- a behavioural pin for the other eight would need a trigger literal per rule, several of them
// method-scans whose shapes have nothing to do with extensions, and a test asserting eight unrelated
// findings to prove one thing about a list is a test that breaks for eight unrelated reasons. Text
// where the fact IS text; behaviour where behaviour is what was doubted.

#[test]
fn the_same_literal_reports_the_same_rules_in_a_jsx_file_as_in_a_ts_file() {
    let dir = TempDir::new("zzop-be-sec-jsx");
    // Split literal, same convention (and same reason) as `vendor_token_committed.rs`'s header.
    let body = concat!(
        "export const API_KEY = \"k7Jx2pQw",
        "9Zr4Tn6Vb8Ly0Mc3Df5Gh1\";\n",
    );
    dir.write("src/config.jsx", body);
    dir.write("src/config.ts", body);
    dir.write("src/config.mts", body);
    dir.write("src/config.cts", body);
    let out = scan(&dir);
    for rule in ["hardcoded-secret", "high-entropy-secret"] {
        let mut files: Vec<&str> = hits(&out, rule).iter().map(|f| f.file.as_str()).collect();
        files.sort_unstable();
        assert_eq!(
            files,
            vec![
                "src/config.cts",
                "src/config.jsx",
                "src/config.mts",
                "src/config.ts"
            ],
            "{rule} must read every extension the TypeScript frontend claims: {:?}",
            out.findings
        );
    }
}

// --- single-file components: which matcher kinds can read a `.vue`/`.svelte` file, and which cannot ---
//
// A `<script setup>` block is ordinary TypeScript TEXT, so a `line-scan` rule needs nothing beyond the
// bytes and every JS/TS line scan in this pack reads SFCs as of 2026-08-20. A `method-scan`/`call-scan`/
// `literal-scan` rule needs the parser, and `zzop_engine`'s `dispatch_by_extension` maps `.vue`/`.svelte`
// to NO language -- so those matcher kinds are deliberately NOT widened FOR SFCs. What their
// `file_pattern` stops at is the extensions a parser actually claims MINUS the SFCs, and that became
// true on 2026-08-21 rather than being true all along: the 2026-08-20 batch gave the line scans
// `jsx|mts|cts` -- which the TypeScript frontend DOES claim -- and left every parser-backed list at
// `ts|tsx|js|mjs|cjs`, so one literal in `src/config.jsx` reported `hardcoded-secret` and nothing
// from `high-entropy-secret` while the byte-identical `src/config.ts` reported both. That was a HOLE,
// not a decision, and the ten parser-backed security rules now carry `jsx|mts|cts` too. The SFC split
// is the one that stays, and the two tests below pin THAT and only that: widening the parser-backed
// lists to SFCs would buy a coverage claim the substrate cannot deliver,
// which is the failure `security/high-entropy-secret`'s own message already records for its `.java` arm.

#[test]
fn a_secret_in_a_vue_script_setup_block_is_flagged_by_the_line_scans() {
    let dir = TempDir::new("zzop-be-sec-vue");
    dir.write(
        "src/A.vue",
        // Split literal, same convention (and same reason) as `vendor_token_committed.rs`'s header.
        concat!(
            "<template><div /></template>\n",
            "<script setup lang=\"ts\">\n",
            "const API_KEY = \"sk_li",
            "ve_51H8xQ2LkjhgfdsaQWERTY123456\";\n",
            "</script>\n",
        ),
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
    assert_eq!(
        hits(&out, "vendor-token-committed").len(),
        1,
        "{:?}",
        out.findings
    );
    // The sibling that reads the parser-projected literal channel CANNOT see it, and that asymmetry is
    // asserted rather than left to be discovered: inside an SFC the line scan above is the only secret
    // channel running, so its shape veto is the only filter there is.
    assert!(
        hits(&out, "high-entropy-secret").is_empty(),
        "no parser projects a `.vue` literal channel: {:?}",
        out.findings
    );
}

#[test]
fn a_secret_in_a_svelte_module_script_is_flagged_by_the_line_scans() {
    let dir = TempDir::new("zzop-be-sec-svelte");
    dir.write(
        "src/routes/Page.svelte",
        concat!(
            "<script lang=\"ts\">\n",
            "  const apiKey = \"9f3Kd0Lm2Qr7Tz4Xb8Vn\";\n",
            "</script>\n",
            "<h1>hi</h1>\n",
        ),
    );
    let out = scan(&dir);
    let h = hits(&out, "hardcoded-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn a_method_scan_rule_stays_silent_in_a_vue_file() {
    // NOT a defect and not a candidate for the same widening: `insecure-cookie` is a `method-scan`, so
    // it needs a projected function body, and no parser dispatches `.vue`. Adding the extension to its
    // `file_pattern` would change nothing except the claim the pattern appears to make. The identical
    // source in a `.ts` file IS flagged -- asserted beside it so this stays a statement about the
    // matcher kind rather than about the fixture.
    let dir = TempDir::new("zzop-be-sec-vue-method");
    dir.write(
        "api/A.vue",
        concat!(
            "<script setup lang=\"ts\">\n",
            "declare const res: any;\n",
            "declare const token: string;\n",
            "export function login() {\n",
            "  res.cookie(\"session\", token);\n",
            "}\n",
            "</script>\n",
        ),
    );
    dir.write(
        "api/auth.ts",
        concat!(
            "declare const res: any;\n",
            "declare const token: string;\n",
            "export function login() {\n",
            "  res.cookie(\"session\", token);\n",
            "}\n",
        ),
    );
    let out = scan(&dir);
    let files: Vec<String> = hits(&out, "insecure-cookie")
        .iter()
        .map(|h| h.file.clone())
        .collect();
    assert_eq!(
        files,
        vec!["api/auth.ts".to_string()],
        "only the parsed file can be judged: {:?}",
        out.findings
    );
}

#[test]
fn a_template_attribute_binding_in_an_sfc_is_not_a_credential() {
    // The one false positive the `.vue`/`.svelte` widening produced on a real tree (koel, 346 SFCs):
    // `<TwoFactorChallengeForm :login-token="twoFactorLoginToken" />` is an attribute BINDING whose
    // value is a bare identifier, not a literal -- and `token` + `=` + a quoted 8+-char run is exactly
    // the `assignment` arm's shape. The veto that answers it is written as the BINDING SHAPE itself
    // (whitespace, `:`, an attribute name, `="`, a bare identifier, `"`), and line 6 is why it is not
    // written as the lowerCamelCase mirror of the PascalCase arm it started life as: that spelling
    // silenced `const password = "myProductionPassword"` too -- a weak literal credential, in the same
    // batch whose message promised weak literal passwords are not silenced. Both a real credential in
    // the SFC's script block (line 5) and a lowerCamelCase one in ordinary assignment position (line 6)
    // are asserted beside the binding, so a later widening of the veto cannot quietly take either.
    let dir = TempDir::new("zzop-be-sec-vue-binding");
    dir.write(
        "src/Auth.vue",
        concat!(
            "<template>\n",
            "  <TwoFactorChallengeForm :login-token=\"twoFactorLoginToken\" />\n",
            "</template>\n",
            "<script setup lang=\"ts\">\n",
            "const token = \"9f3Kd0Lm2Qr7Tz4Xb8Vn\";\n",
            "const password = \"myProductionPassword\";\n",
            "</script>\n",
        ),
    );
    let out = scan(&dir);
    let lines: Vec<u32> = hits(&out, "hardcoded-secret")
        .iter()
        .map(|h| h.line)
        .collect();
    assert_eq!(lines, vec![5, 6], "{:?}", out.findings);
}
