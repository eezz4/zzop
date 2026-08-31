//! End-to-end tests for NUXT AUTO-IMPORT liveness (`analyze::assemble::nuxt_auto_import::scan` ->
//! `analyze::assemble::dep_graph::fan_in::merge_auto_import_fan_in` -> `dead-candidates`' fan-in, plus
//! the `unreachable` `extra_entries` seed in `analyze::assemble::rules`).
//!
//! A file under a Nuxt auto-import directory is reached by BARE SYMBOL NAME with no import statement
//! anywhere, so the dep graph sees no importer and `dead-candidates` calls it dead. Measured on nocodb
//! 3a5cbd5: 207 of 296 findings sat in those directories, 189 of them false. Everything below runs the
//! real pipeline (`analyze_tree`, real source on disk) and each test names the pin it holds:
//! - a `.vue` `<template>`-only mention resolves (the token scan reads the whole file, not `<script>`),
//! - a `.ts` call site resolves through the fused pass's own `used_names` roster,
//! - a genuinely unreferenced composable is STILL reported (never-over-suppress — the clause that
//!   separates a resolution from a directory exemption),
//! - the same directory shape with NO `nuxt.config.*` present resolves nothing (the anchor),
//! - a reference from OUTSIDE the Nuxt app dir resolves nothing (the scope wall — measured: a
//!   whole-repo scan silences 2 of nocodb's 18 genuinely dead files),
//! - a reference from the app's OWN `public/` resolves nothing (that directory is served verbatim, so
//!   the app-dir wall alone does not exclude the vendored bundle sitting in it),
//! - a `.md` page does not vouch (only `.vue` carries a whole-text token roster),
//! - one app's call site does not vouch for ANOTHER app's composable (each Nuxt app is built with its
//!   own auto-import table),
//! - a silenced file does not flip into a false `unreachable` island (the `extra_entries` seed).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, EngineConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "fixture".to_string(),
        ..EngineConfig::default()
    }
}

fn dead_candidates(out: &zzop_engine::AnalyzeOutput) -> Vec<String> {
    let mut v: Vec<String> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "dead-candidates")
        .map(|f| f.file.clone())
        .collect();
    v.sort();
    v
}

fn unreachable_files(out: &zzop_engine::AnalyzeOutput) -> Vec<String> {
    let mut v: Vec<String> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "unreachable")
        .map(|f| f.file.clone())
        .collect();
    v.sort();
    v
}

/// The measured nocodb shape, miniaturized: a Nuxt app nested in a monorepo whose composables are
/// reached three different ways — a `<template>` mention, a `<script>` mention, and a `.ts` call site —
/// plus one composable nothing mentions at all.
fn write_app(dir: &TempDir) {
    dir.write("app/nuxt.config.ts", "export default { ssr: true }\n");
    dir.write(
        "app/composables/useTemplateOnly.ts",
        "export const useTemplateOnly = () => 1\n",
    );
    dir.write(
        "app/composables/useScriptOnly.ts",
        "export const useScriptOnly = () => 2\n",
    );
    dir.write(
        "app/utils/fromTs.ts",
        "export function fromTs() { return 3 }\n",
    );
    dir.write(
        "app/composables/useNobodyCalls.ts",
        "export const useNobodyCalls = () => 4\n",
    );
    dir.write(
        "app/pages/index.vue",
        "<script setup>\nconst n = useScriptOnly()\n</script>\n<template>\n  <div>{{ useTemplateOnly() }}</div>\n</template>\n",
    );
    // A plain `.ts` call site inside the app: its reference roster is the fused pass's `used_names`,
    // not a raw token scan, so this arm is a different code path from the `.vue` one above.
    dir.write("app/plugins/boot.ts", "export default () => fromTs()\n");
}

/// The core repair, and the never-over-suppress control in the same run: three files reached only by
/// bare name go quiet, and the fourth — mentioned nowhere — is still named individually.
#[test]
fn bare_name_references_clear_dead_candidates_and_an_unreferenced_one_still_reports() {
    let dir = TempDir::new("zzop-engine-nuxt-auto-import");
    write_app(&dir);
    let out = analyze_tree(dir.path(), &config());
    let found = dead_candidates(&out);

    for reached in [
        "app/composables/useTemplateOnly.ts",
        "app/composables/useScriptOnly.ts",
        "app/utils/fromTs.ts",
    ] {
        assert!(
            !found.iter().any(|f| f == reached),
            "{reached} is auto-imported and referenced by bare name — it must not be a dead \
             candidate, got: {found:?}"
        );
    }
    assert!(
        found
            .iter()
            .any(|f| f == "app/composables/useNobodyCalls.ts"),
        "a composable nothing mentions must STILL be reported by name — an exemption of the \
         directory would produce nothing here, got: {found:?}"
    );
}

/// The seed into `unreachable`'s `extra_entries`. Without it every file the fan-in bump silences flips
/// from a `dead-candidates` false positive into a false `unreachable` island — the same defect wearing
/// a different rule id — because a bare name creates no `dep` edge for the reachability walk to follow.
#[test]
fn a_cleared_auto_import_target_is_not_a_false_unreachable_island() {
    let dir = TempDir::new("zzop-engine-nuxt-auto-import-unreachable");
    write_app(&dir);
    // The composable pulls in a helper by a REAL import, so the helper is reachable only THROUGH the
    // auto-imported file: if the target is not seeded as an entry, the helper reads as an island too.
    dir.write("app/lib/helper.ts", "export const helper = () => 9\n");
    dir.write(
        "app/composables/useScriptOnly.ts",
        "import { helper } from '../lib/helper'\nexport const useScriptOnly = () => helper()\n",
    );
    let out = analyze_tree(dir.path(), &config());
    let islands = unreachable_files(&out);

    for reached in ["app/composables/useScriptOnly.ts", "app/lib/helper.ts"] {
        assert!(
            !islands.iter().any(|f| f == reached),
            "{reached} is reached through a Nuxt auto-import, which the dep graph cannot see — it \
             must be seeded as an unreachable entry, got: {islands:?}"
        );
    }
}

/// The ANCHOR. The exact same directory layout with no `nuxt.config.*` in it is an ordinary tree, and
/// silencing `composables/` there would hide real dead code. This is also the pin that keeps every
/// non-Nuxt corpus tree at its old numbers.
#[test]
fn without_a_nuxt_config_the_same_layout_resolves_nothing() {
    let dir = TempDir::new("zzop-engine-nuxt-auto-import-no-config");
    write_app(&dir);
    fs::remove_file(dir.path().join("app/nuxt.config.ts")).unwrap();
    let out = analyze_tree(dir.path(), &config());
    let found = dead_candidates(&out);

    for still_dead in [
        "app/composables/useTemplateOnly.ts",
        "app/composables/useScriptOnly.ts",
        "app/utils/fromTs.ts",
    ] {
        assert!(
            found.iter().any(|f| f == still_dead),
            "with no nuxt.config.* there is no auto-import table, so {still_dead} must stay a dead \
             candidate, got: {found:?}"
        );
    }
}

/// The SCOPE WALL, which is the design rather than a tidiness rule. Measured on nocodb: a whole-repo
/// bare-name scan silences 2 of the 17 genuinely dead files — `utils/mimeTypeUtils.ts` dies to a
/// different `mimeIcons` in `packages/nocodb/src`, and `utils/workflowUtils.ts` to a `transformNode`
/// token inside a minified vendored `vue.min.js`. Both are outside the Nuxt app dir.
#[test]
fn a_reference_from_outside_the_app_dir_does_not_vouch_for_a_composable() {
    let dir = TempDir::new("zzop-engine-nuxt-auto-import-scope");
    dir.write("app/nuxt.config.ts", "export default { ssr: true }\n");
    dir.write(
        "app/utils/mimeIcons.ts",
        "export const mimeIcons = { a: 1 }\n",
    );
    dir.write("app/entry.ts", "export const entry = 1\n");
    // A SIBLING package, outside the app dir, declaring its own unrelated `mimeIcons`.
    dir.write(
        "server/src/attachments.ts",
        "const mimeIcons = { b: 2 }\nexport const pick = () => mimeIcons\n",
    );
    let out = analyze_tree(dir.path(), &config());
    let found = dead_candidates(&out);

    assert!(
        found.iter().any(|f| f == "app/utils/mimeIcons.ts"),
        "the only mention of `mimeIcons` is in a sibling package outside the Nuxt app dir — it must \
         not vouch for the app's own util, got: {found:?}"
    );
}

/// The app's OWN `public/`, which the app-dir wall cannot reach. Nuxt copies that directory to the
/// site root untransformed, so the auto-import table is never applied to anything in it and no token
/// there is a call site. Measured on nocodb: `nc-gui/public/js/swagger-ui-bundle.min.js` is 1.06 MB and
/// 8898 distinct tokens, sitting inside the Nuxt app dir.
#[test]
fn a_reference_from_the_apps_own_public_dir_does_not_vouch() {
    let dir = TempDir::new("zzop-engine-nuxt-auto-import-public");
    dir.write("app/nuxt.config.ts", "export default { ssr: true }\n");
    dir.write(
        "app/composables/useVendored.ts",
        "export const useVendored = () => 1\n",
    );
    dir.write(
        "app/composables/useReal.ts",
        "export const useReal = () => 2\n",
    );
    // A vendored bundle the app ships itself. Its only job here is to mention the composable's name.
    dir.write(
        "app/public/js/vendor.min.js",
        "export const boot = () => useVendored()\n",
    );
    // The control, in the same run: an ordinary in-app call site still resolves, so a green assertion
    // above cannot come from the mechanism being off.
    dir.write("app/plugins/boot.ts", "export default () => useReal()\n");
    let found = dead_candidates(&analyze_tree(dir.path(), &config()));

    assert!(
        found.iter().any(|f| f == "app/composables/useVendored.ts"),
        "the only mention of `useVendored` is in the app's own `public/`, which Nuxt serves verbatim \
         and never transforms — it must not vouch, got: {found:?}"
    );
    assert!(
        !found.iter().any(|f| f == "app/composables/useReal.ts"),
        "an ordinary in-app `.ts` call site must still resolve, got: {found:?}"
    );
}

/// Only `.vue` carries a whole-text token roster. The import pre-scan also reads `.md`/`.mdx` (Nuxt
/// Content compiles them) but only their `<script setup>` block is code — the prose is not a call site,
/// and a whole-text scan cannot tell the two apart. Measured on nocodb: its 7 `.md` files vouch for 0
/// of the 189 resolved subjects, so dropping them costs that tree nothing.
#[test]
fn a_markdown_page_does_not_vouch_for_a_composable() {
    let dir = TempDir::new("zzop-engine-nuxt-auto-import-md");
    dir.write("app/nuxt.config.ts", "export default { ssr: true }\n");
    dir.write(
        "app/composables/useProseOnly.ts",
        "export const useProseOnly = () => 1\n",
    );
    dir.write(
        "app/composables/useTemplateOnly.ts",
        "export const useTemplateOnly = () => 2\n",
    );
    dir.write(
        "app/content/guide.md",
        "# Guide\n\nCall useProseOnly() to read the thing.\n",
    );
    // The control: a `.vue` template mention DOES vouch, so the assertion above is about the filetype
    // and not about the token scan being dead.
    dir.write(
        "app/pages/index.vue",
        "<template>\n  <div>{{ useTemplateOnly() }}</div>\n</template>\n",
    );
    let found = dead_candidates(&analyze_tree(dir.path(), &config()));

    assert!(
        found.iter().any(|f| f == "app/composables/useProseOnly.ts"),
        "a `.md` page's prose is not an auto-import call site — it must not vouch, got: {found:?}"
    );
    assert!(
        !found
            .iter()
            .any(|f| f == "app/composables/useTemplateOnly.ts"),
        "a `.vue` template mention must still resolve, got: {found:?}"
    );
}

/// The wall is PER APP, not a union over the app dirs. Nuxt builds each app with its own auto-import
/// table, so a bare name written in `apps/web` resolves nothing in `apps/admin` — and a union would
/// both bump the admin file's fan-in and seed it into `unreachable`'s `extra_entries` on the strength
/// of a table it is never compiled against. `dead-candidates` is where that is OBSERVABLE: this fixture
/// has no entry point at all, so `unreachable` reports nothing here either way and cannot be the pin.
/// (Nuxt LAYERS do share tables through `extends`; that is not modelled and this fixture does not use
/// it.)
#[test]
fn one_apps_reference_does_not_vouch_for_another_apps_composable() {
    let dir = TempDir::new("zzop-engine-nuxt-auto-import-two-apps");
    dir.write("apps/web/nuxt.config.ts", "export default { ssr: true }\n");
    dir.write(
        "apps/admin/nuxt.config.ts",
        "export default { ssr: true }\n",
    );
    dir.write(
        "apps/admin/composables/useUser.ts",
        "export const useUser = () => 1\n",
    );
    dir.write(
        "apps/web/composables/useWebOnly.ts",
        "export const useWebOnly = () => 2\n",
    );
    dir.write(
        "apps/web/pages/index.vue",
        "<template>\n  <div>{{ useUser() }} {{ useWebOnly() }}</div>\n</template>\n",
    );
    let found = dead_candidates(&analyze_tree(dir.path(), &config()));

    assert!(
        found.iter().any(|f| f == "apps/admin/composables/useUser.ts"),
        "`useUser` is written only in the OTHER app, whose auto-import table the admin build never \
         sees — it must not vouch, got: {found:?}"
    );
    assert!(
        !found
            .iter()
            .any(|f| f == "apps/web/composables/useWebOnly.ts"),
        "the same page's mention of its OWN app's composable must still resolve, got: {found:?}"
    );
}

/// The DECLARED half (§24). `store/` is not a Nuxt convention: nothing but the app's own
/// `nuxt.config.*` line makes it an auto-import directory, so the same file goes quiet with the
/// declaration present and stays reported without it. Measured on nocodb, that one line is the sole
/// reason 34 files under `store`/`helpers`/`lib` are reachable at all.
#[test]
fn a_declared_imports_dir_is_read_and_an_undeclared_one_is_not() {
    let with_decl = TempDir::new("zzop-engine-nuxt-auto-import-declared");
    with_decl.write(
        "app/nuxt.config.ts",
        "export default defineNuxtConfig({\n  imports: { dirs: ['./store/**'] },\n})\n",
    );
    with_decl.write("app/store/base.ts", "export const useBaseStore = () => 1\n");
    with_decl.write(
        "app/store/deep/rows.ts",
        "export const useRowsStore = () => 2\n",
    );
    with_decl.write("app/helpers/fmt.ts", "export const fmtThing = () => 3\n");
    with_decl.write(
        "app/pages/index.vue",
        "<template>\n  <div>{{ useBaseStore() }} {{ useRowsStore() }} {{ fmtThing() }}</div>\n</template>\n",
    );
    let found = dead_candidates(&analyze_tree(with_decl.path(), &config()));

    for cleared in ["app/store/base.ts", "app/store/deep/rows.ts"] {
        assert!(
            !found.iter().any(|f| f == cleared),
            "`./store/**` is declared, so {cleared} is auto-imported and referenced, got: {found:?}"
        );
    }
    assert!(
        found.iter().any(|f| f == "app/helpers/fmt.ts"),
        "`helpers` is NOT a Nuxt convention and this config does not declare it — a bare-name mention \
         must not clear it, got: {found:?}"
    );

    // The control: same tree, declaration removed. `store/` is an ordinary directory again.
    let no_decl = TempDir::new("zzop-engine-nuxt-auto-import-undeclared");
    no_decl.write(
        "app/nuxt.config.ts",
        "export default defineNuxtConfig({})\n",
    );
    no_decl.write("app/store/base.ts", "export const useBaseStore = () => 1\n");
    no_decl.write(
        "app/pages/index.vue",
        "<template>\n  <div>{{ useBaseStore() }}</div>\n</template>\n",
    );
    let found = dead_candidates(&analyze_tree(no_decl.path(), &config()));
    assert!(
        found.iter().any(|f| f == "app/store/base.ts"),
        "with no `imports.dirs` line, `store/` is not auto-imported and the file must stay a dead \
         candidate, got: {found:?}"
    );
}

/// Nuxt's default scan is `<dir>/*` plus `<dir>/*/index.*`; a deeper file is auto-imported only when
/// `imports.dirs` declares a recursive glob. Under-claiming there is a missing repair; over-claiming
/// would be a silenced live-looking file the tree never auto-imported.
#[test]
fn a_nested_non_index_file_is_not_a_default_candidate() {
    let dir = TempDir::new("zzop-engine-nuxt-auto-import-nesting");
    dir.write("app/nuxt.config.ts", "export default { ssr: true }\n");
    dir.write(
        "app/composables/deep/inner.ts",
        "export const inner = () => 1\n",
    );
    dir.write(
        "app/composables/wrapped/index.ts",
        "export const wrapped = () => 2\n",
    );
    dir.write(
        "app/pages/index.vue",
        "<template>\n  <div>{{ inner() }} {{ wrapped() }}</div>\n</template>\n",
    );
    let out = analyze_tree(dir.path(), &config());
    let found = dead_candidates(&out);

    assert!(
        found.iter().any(|f| f == "app/composables/deep/inner.ts"),
        "a nested non-index file is outside Nuxt's DEFAULT auto-import scan, so a bare-name mention \
         must not clear it, got: {found:?}"
    );
    assert!(
        !found
            .iter()
            .any(|f| f == "app/composables/wrapped/index.ts"),
        "`<dir>/*/index.*` IS a Nuxt default auto-import shape and must be cleared, got: {found:?}"
    );
}
