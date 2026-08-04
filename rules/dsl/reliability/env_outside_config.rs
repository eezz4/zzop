use crate::{env_config_overlay, hits, scan, scan_with, warnings_matching, TempDir};

// --- env-outside-config ---
//
// The rule consumes a DECLARATION instead of guessing one. `env-config-module` is injected the same way
// every other cross-cutting fact is (`AttributeStore` <- a Mode-B overlay's per-file `attributes`), and
// the matcher gates on it twice:
//   `attr_absent: env-config-module`          -> a file covered by a truthy declaration is exempt
//   `require_attr_declared: env-config-module` -> nothing declared anywhere = the rule does not run
//
// What this replaced, and why it is gone: a basename regex (`config|env|constants`) plus two whole-file
// syntax fragments (`env-accessor-module` / `env-wrapper-module`) that tried to infer which file was the
// config module from its shape. Both were guesses about the environment, which this engine's soundness
// floor forbids; the rule's own old message admitted the decisive one ("a whole-tree fact no file-local
// matcher can establish"). The trade is real and deliberate: a zero-config user now gets silence plus a
// disclosure instead of a partly-right answer.

/// Asserts the rule fires exactly once in `src`, on `line`, with `src/config` declared as the env module.
fn fires(path: &str, src: &str, line: u32) {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(path, src);
    let out = scan_with(&dir, env_config_overlay(&["src/config"]));
    let h = hits(&out, "env-outside-config");
    assert_eq!(h.len(), 1, "{path}: {:?}", out.findings);
    assert_eq!(h[0].line, line, "{path}: {:?}", out.findings);
}

/// Asserts the rule is silent on `src` under that same declaration.
fn exempt(path: &str, src: &str) {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(path, src);
    let out = scan_with(&dir, env_config_overlay(&["src/config"]));
    let h = hits(&out, "env-outside-config");
    assert!(h.is_empty(), "{path}: {:?}", out.findings);
}

// --- the declaration gate (the contract this rule now rests on) ---

#[test]
fn with_nothing_declared_the_rule_is_silent_and_the_run_discloses_why() {
    // THE trade, pinned in both halves: no findings, AND a warning that names the rule, the missing
    // vocabulary, how much was left unjudged, and the two ways forward. Silence alone would read as
    // "clean" to the agent this tool is written for — the failure mode this repo treats as cardinal.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/app/page.tsx",
        "export default function Page() {\n  return process.env.NEXT_PUBLIC_TITLE;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "undeclared must not fall back to guessing: {:?}",
        out.findings
    );
    let disclosed = warnings_matching(&out, "env-outside-config");
    assert_eq!(disclosed.len(), 1, "warnings: {:?}", out.warnings);
    let w = disclosed[0];
    assert!(w.contains("env-config-module"), "names the vocabulary: {w}");
    assert!(w.contains("1 candidate site"), "states the volume: {w}");
    assert!(w.contains("overlays"), "says how to declare it: {w}");
    assert!(w.contains("\"off\""), "says how to opt out instead: {w}");
}

#[test]
fn a_tree_with_no_env_reads_at_all_discloses_nothing() {
    // Boundary of the disclosure: "the rule had nothing to say" is a real zero and must stay quiet, or
    // the warning that matters gets trained away as noise.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/app/page.tsx",
        "export default function Page() {\n  return 1;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        warnings_matching(&out, "env-outside-config").is_empty(),
        "warnings: {:?}",
        out.warnings
    );
}

#[test]
fn declaring_the_config_module_re_enables_the_rule_and_exempts_that_module() {
    // The other half of the same run: the declaration both turns the rule on and carves out the declared
    // paths. Asserted together so a change that only does one of the two cannot pass.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/config/env.ts",
        "export const port = process.env.PORT;\n",
    );
    dir.write(
        "src/app/page.tsx",
        "export default function Page() {\n  return process.env.NEXT_PUBLIC_TITLE;\n}\n",
    );
    let out = scan_with(&dir, env_config_overlay(&["src/config"]));
    let h = hits(&out, "env-outside-config");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].file, "src/app/page.tsx");
    assert!(
        warnings_matching(&out, "env-outside-config").is_empty(),
        "a rule that ran discloses nothing: {:?}",
        out.warnings
    );
}

#[test]
fn the_declared_scope_covers_a_whole_directory_on_segment_boundaries() {
    // Why a scope and not a file list: nobody maintains a per-file declaration. `src/config` covers
    // everything under it and nothing that merely starts with the same letters.
    exempt(
        "src/config/nested/database.ts",
        "export const dbUrl = process.env.DATABASE_URL;\n",
    );
    fires(
        "src/configuration.ts",
        "export const dbUrl = process.env.DATABASE_URL;\n",
        1,
    );
}

#[test]
fn a_name_that_merely_looks_like_config_is_no_longer_exempt() {
    // The deleted guess, asserted as deleted. These four paths were all exempt purely because of what
    // they were CALLED; now they fire unless the project says otherwise. This is the intended behavior
    // change, not collateral — a `constants.ts` that reads env is only fine if someone declares it so.
    fires(
        "src/config.ts",
        "export const port = process.env.PORT;\n",
        1,
    );
    fires(
        "packages/lib/constants.ts",
        "export const WEBAPP_URL = process.env.NEXT_PUBLIC_WEBAPP_URL;\n",
        1,
    );
    fires(
        "apps/web/next.config.mjs",
        "export default { env: { API_URL: process.env.API_URL } };\n",
        1,
    );
    fires(
        ".eslintrc.js",
        "module.exports = { rules: process.env.STRICT ? {} : {} };\n",
        1,
    );
}

#[test]
fn a_single_accessor_module_is_no_longer_exempt_by_shape() {
    // Corpus byte-shape of `apps/*-hub-fe/src/lib/apiBase.ts`, previously exempt via the deleted
    // `env-accessor-module` fragment (a file whose entire surface is one short sync lowercase-named
    // `export function`). The shape proved nothing about the project's actual config seam — it is now
    // either declared or flagged, and there is no third answer.
    fires(
        "src/lib/apiBase.ts",
        "/* eslint-disable local/no-direct-env */\n/** BE base URL, injected per environment. */\nexport function apiBase(): string {\n  return process.env.NEXT_PUBLIC_PING_API_URL ?? \"\";\n}\n",
        4,
    );
    exempt_declared(
        "src/lib/apiBase.ts",
        "/** BE base URL, injected per environment. */\nexport function apiBase(): string {\n  return process.env.NEXT_PUBLIC_PING_API_URL ?? \"\";\n}\n",
    );
}

/// `exempt`'s twin for a file declared by its own exact path rather than by a covering directory — the
/// spelling a project uses when its config seam is one file, not a folder.
fn exempt_declared(path: &str, src: &str) {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(path, src);
    let out = scan_with(&dir, env_config_overlay(&[path]));
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "{path}: {:?}",
        out.findings
    );
}

#[test]
fn an_exact_file_declaration_beats_a_covering_scope() {
    // Specificity, end to end: `src/app` declared wholesale, one route inside it carved back out. This is
    // the escape hatch that keeps a directory-level declaration usable on a real tree.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/app/admin/route.ts",
        "export async function GET() {\n  return new Response(process.env.ADMIN_SECRET);\n}\n",
    );
    let mut overlay = env_config_overlay(&["src/app"]);
    crate::deny_env_config_for_file(&mut overlay, "src/app/admin/route.ts");
    let out = scan_with(&dir, overlay);
    let h = hits(&out, "env-outside-config");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

// --- exclusions that survive independently of any declaration ---

#[test]
fn env_access_in_a_test_or_story_path_is_not_flagged() {
    // A code-organization rule, not a security one: a test fixture reading env directly is not the
    // scattering across application code this rule targets. Kept as the shared `${test-paths-stories}`
    // vocabulary rather than this rule's own copy of it.
    exempt(
        "src/handler.test.ts",
        "it('reads a var', () => {\n  expect(process.env.PORT).toBeDefined();\n});\n",
    );
    exempt(
        "src/ui/Banner.stories.tsx",
        "export const Default = { args: { title: process.env.NEXT_PUBLIC_TITLE } };\n",
    );
}

#[test]
fn a_one_off_script_now_fires_where_the_bespoke_exclusion_used_to_hide_it() {
    // Behavior change, pinned rather than left to surface as a surprise: the old hand-written exclusion
    // carried a `scripts?/` clause that the shared test-path vocabulary does not. A seed script reading
    // env directly IS application code by this rule's premise; a project that disagrees declares it or
    // excludes it in config.
    fires(
        "scripts/seed.ts",
        "async function seed() {\n  const url = process.env.DATABASE_URL;\n  console.log(url);\n}\n",
        2,
    );
}

// --- ordinary scattered reads ---

#[test]
fn component_and_route_handler_shapes_fire() {
    fires(
        "src/app/page.tsx",
        "export default function Page() {\n  return process.env.NEXT_PUBLIC_TITLE;\n}\n",
        2,
    );
    fires(
        "src/ui/Banner.tsx",
        "export function Banner() {\n  return <div>{process.env.NEXT_PUBLIC_BANNER}</div>;\n}\n",
        2,
    );
}

#[test]
fn an_env_read_in_a_private_helper_of_a_route_file_fires() {
    // Corpus pin, `apps/interest-hub-fe/src/app/(explore)/feed.xml/route.ts:37` — the genuine finding of
    // the original six, and the one measured true positive on mono-hub before this rework. It must
    // survive the rework, which is the whole point of measuring the declared case too.
    fires(
        "src/app/feed.xml/route.ts",
        "import { loadCells } from \"./loadCells\";\n\nexport function GET() {\n  return new Response(renderRss());\n}\n\nfunction renderRss(): string {\n  const site = process.env.NEXT_PUBLIC_SITE_URL ?? \"https://x.example\";\n  return site;\n}\n",
        8,
    );
}

// --- what the call-scan migration changed, in all three directions ---
//
// The tests above are the PARITY set — every one of them passed against the line-scan version and must
// keep passing, which is what makes the delta below attributable. These pin the three ways the reach
// moved when "what counts as a read" became a projected fact instead of a regex.

#[test]
fn the_bracket_forms_the_old_regex_could_not_see_now_fire() {
    // The WIDENING, and the one the migration's detection delta has to account for. `\bprocess\.env\.`
    // required a dot and a bare identifier, so a quoted key and a computed key were both invisible. The
    // producer emits for both: the callee (`process.env`) is fully resolved and only the KEY is dynamic,
    // and the key is not a field of this channel — so emitting guesses nothing.
    fires(
        "src/handler.ts",
        "export const port = process.env[\"PORT\"];\n",
        1,
    );
    fires(
        "src/handler.ts",
        "export function read(k: string) {\n  return process.env[k];\n}\n",
        2,
    );
}

#[test]
fn two_reads_on_one_line_are_now_two_findings() {
    // A per-SITE rule where the old one was per-LINE. Worth pinning rather than discovering from a
    // corpus count that moved: the same source produces a different NUMBER of findings, with no change
    // in which files are implicated.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/handler.ts",
        "export const dsn = `${process.env.HOST}:${process.env.PORT}`;\n",
    );
    let out = scan_with(&dir, env_config_overlay(&["src/config"]));
    let h = hits(&out, "env-outside-config");
    assert_eq!(h.len(), 2, "{:?}", out.findings);
    assert!(h.iter().all(|f| f.line == 1), "{:?}", out.findings);
}

#[test]
fn a_bare_process_env_with_no_key_is_still_not_a_read() {
    // The boundary that did NOT move, asserted so the widening above is not read as "everything now
    // fires". `const e = process.env` names no key at the site; the producer is silent, exactly as the
    // old regex was, so this population is neither gained nor lost.
    exempt(
        "src/handler.ts",
        "const all = process.env;\nexport const keys = Object.keys(all);\n",
    );
}

#[test]
fn python_env_reads_now_fire_in_all_three_spellings() {
    // The REACH gain. One rule covers both languages because the channel does; the three spellings are
    // the producer's recognized set, and `os.environ[...]` is the Python twin of the TypeScript bracket
    // form above.
    fires("src/handler.py", "PORT = os.getenv(\"PORT\")\n", 1);
    fires("src/handler.py", "PORT = os.environ.get(\"PORT\")\n", 1);
    fires("src/handler.py", "PORT = os.environ[\"PORT\"]\n", 1);
}

#[test]
fn a_python_hash_ok_marker_suppresses_the_finding() {
    exempt(
        "src/handler.py",
        "# zzop-env-outside-config-ok: bootstrap shim, migration tracked\nPORT = os.getenv(\"PORT\")\n",
    );
}

#[test]
fn a_read_named_only_in_a_string_or_a_comment_is_no_longer_a_read() {
    // The NARROWING, and the honest half of the trade. The old regex had `skip_comment_lines` for the
    // first of these and nothing at all for the second; neither is a site now.
    exempt(
        "src/handler.ts",
        "// process.env.PORT is read in config/env.ts\nexport const hint = \"set process.env.PORT before boot\";\n",
    );
}

// --- suppression + rule interplay ---

#[test]
fn env_access_ok_marker_above_the_line_suppresses_the_finding() {
    exempt(
        "src/handler.ts",
        "const NAME = \"svc\";\n\nexport function getPort() {\n  // zzop-env-outside-config-ok: legacy call site, migration tracked in JIRA-123\n  return `${NAME}:${process.env.PORT}`;\n}\n",
    );
}

#[test]
fn process_env_nonnull_assertion_outside_config_fires_both_env_rules_on_the_same_line() {
    // Documented interplay: env-nonnull-assert (deferred-crash risk of `!`) and env-outside-config
    // (scattered env access) are different concerns, so both firing on one line is intended. Also the
    // pin that the SIBLING rule is unaffected by the declaration gate — it has none, and an undeclared
    // run must not silence it by association.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/handler.ts",
        "export const key = process.env.API_KEY!;\n",
    );
    let undeclared = scan(&dir);
    assert_eq!(
        hits(&undeclared, "env-nonnull-assert").len(),
        1,
        "{:?}",
        undeclared.findings
    );
    assert!(hits(&undeclared, "env-outside-config").is_empty());

    let out = scan_with(&dir, env_config_overlay(&["src/config"]));
    assert_eq!(
        hits(&out, "env-nonnull-assert").len(),
        1,
        "{:?}",
        out.findings
    );
    assert_eq!(
        hits(&out, "env-outside-config").len(),
        1,
        "{:?}",
        out.findings
    );
}
