//! Unit tests for the tool-config entry harvest. Every fixture below is written to a real temp tree,
//! because `scan` reads the config off disk and resolves against the run's own file universe — a
//! test that stubbed either half would prove nothing about the pass that ships.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::scan;

struct TempTree(PathBuf);

impl TempTree {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "zzop-config-entries-{}-{nanos}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        TempTree(dir)
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

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn paths(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The koel 3f5213d4 shape, both configs at once, ASSERTED TOGETHER WITH THE FILE THAT MUST STAY
/// FLAGGED. The whole point of this pass is to delete findings, so a test that only proved the three
/// entry files were excused would also pass if the harvest excused the entire tree.
///
/// - `laravel({ input: [...] })` — a positional argument to a plugin CALL, in the root config.
/// - `build.lib.entry: resolve(__dirname, …)` — a nested key wrapped in a call, in a SECOND config
///   reachable only through the tool's `--config` flag.
/// - `resources/assets/js/config/acceptedImageTypes.ts` — named by no config, and independently
///   judged genuinely dead by two auditors (whole-repo grep: one hit, its own definition). It must
///   NOT be harvested.
#[test]
fn both_koel_configs_yield_their_entries_and_nothing_they_do_not_name() {
    let t = TempTree::new();
    t.write(
        "vite.config.ts",
        r#"
        import laravel from 'laravel-vite-plugin'
        import { resolve } from 'path'
        export default defineConfig({
          plugins: [
            laravel({
              input: ['resources/assets/js/app.ts', 'resources/assets/js/remote/app.ts'],
              refresh: true,
            }),
          ],
          resolve: { alias: { '@': resolve(__dirname, './resources/assets/js') } },
        })
        "#,
    );
    t.write(
        "vite.config.sw.js",
        r#"
        import { resolve } from 'path'
        export default defineConfig({
          build: {
            lib: { entry: resolve(__dirname, 'resources/assets/js/service-worker.ts') },
            outDir: 'public',
          },
        })
        "#,
    );
    let universe = paths(&[
        "vite.config.ts",
        "vite.config.sw.js",
        "resources/assets/js/app.ts",
        "resources/assets/js/remote/app.ts",
        "resources/assets/js/service-worker.ts",
        "resources/assets/js/config/acceptedImageTypes.ts",
    ]);

    let found = scan(t.path(), &universe);

    assert!(found.contains("resources/assets/js/app.ts"), "{found:?}");
    assert!(
        found.contains("resources/assets/js/remote/app.ts"),
        "{found:?}"
    );
    assert!(
        found.contains("resources/assets/js/service-worker.ts"),
        "{found:?}"
    );
    assert!(
        !found.contains("resources/assets/js/config/acceptedImageTypes.ts"),
        "a file no config names must stay a dead candidate: {found:?}"
    );
}

/// The alias table is the reason the extension is required. `'@': resolve(__dirname,
/// './resources/assets/js')` names the application's whole source ROOT; if a directory string could
/// resolve, this one exemption would excuse every file under it and the rule would be over.
#[test]
fn a_directory_string_with_no_extension_resolves_to_nothing() {
    let t = TempTree::new();
    t.write(
        "vite.config.ts",
        "export default { resolve: { alias: { '@': './src', '~': 'src/lib' } } }",
    );
    let universe = paths(&["vite.config.ts", "src/index.ts", "src/lib/util.ts"]);
    assert!(scan(t.path(), &universe).is_empty());
}

/// A path that resolves relative to the CONFIG's directory, not the tree root — a monorepo writes the
/// same `./src/main.ts` in every package and means a different file each time.
#[test]
fn a_nested_config_resolves_against_its_own_directory() {
    let t = TempTree::new();
    t.write(
        "packages/web/vite.config.ts",
        "export default { build: { rollupOptions: { input: './src/main.ts' } } }",
    );
    let universe = paths(&[
        "packages/web/vite.config.ts",
        "packages/web/src/main.ts",
        "src/main.ts",
    ]);
    let found = scan(t.path(), &universe);
    assert_eq!(
        found,
        paths(&["packages/web/src/main.ts"]),
        "the sibling root-level src/main.ts must not be excused: {found:?}"
    );
}

/// A config naming a COMPILED path still lands on the source that produces it — the same `try_ext`
/// fallback the `package.json` entry scan relies on, and the reason a `dist`-flavored entry string is
/// not silently useless.
#[test]
fn a_js_spelling_resolves_to_the_ts_file_behind_it() {
    let t = TempTree::new();
    t.write(
        "build.config.ts",
        "export default { entry: './src/index.js' }",
    );
    let universe = paths(&["build.config.ts", "src/index.ts"]);
    assert_eq!(scan(t.path(), &universe), paths(&["src/index.ts"]));
}

/// A path string that names no file in this tree — an external package's own entry, a stale reference,
/// an absolute host path — must mint nothing. `try_ext` is the only admission gate, so "resolves"
/// always means "this tree has this file".
#[test]
fn a_path_that_names_no_file_in_the_tree_mints_nothing() {
    let t = TempTree::new();
    t.write(
        "vite.config.ts",
        r#"export default { entry: './src/gone.ts', other: '/abs/elsewhere/x.ts', pkg: 'some-lib/dist/index.js' }"#,
    );
    let universe = paths(&["vite.config.ts", "src/kept.ts"]);
    assert!(scan(t.path(), &universe).is_empty());
}

/// Only files the config predicate admits are OPENED. An ordinary module that happens to contain a
/// path-shaped string is not a declaration source, and reading it would turn every string constant in
/// the tree into an exemption.
#[test]
fn an_ordinary_module_is_never_read_as_a_declaration_source() {
    let t = TempTree::new();
    t.write("src/registry.ts", "export const LEGACY = './src/orphan.ts'");
    let universe = paths(&["src/registry.ts", "src/orphan.ts"]);
    assert!(scan(t.path(), &universe).is_empty());
}

/// **The polarity cost, pinned so it cannot change by accident.** The harvest matches a VALUE shape
/// and never reads the KEY, so a path in an EXCLUSION array is exempted exactly like an entry. This
/// test asserts that cost rather than a desired behaviour: the exemption runs in the erasing
/// direction, the module doc and both user-facing surfaces state it in those words, and if a later
/// edit makes the harvest polarity-aware this test is the one that has to be rewritten deliberately.
///
/// All three shapes ride in one call so the assertion carries its own contrast: the entry that SHOULD
/// be excused, the exclusion path that IS excused as a known cost, and the file no config mentions,
/// which must stay a candidate whatever else changes.
#[test]
fn a_path_in_an_exclusion_array_is_exempted_exactly_like_an_entry() {
    let t = TempTree::new();
    t.write(
        "vitest.config.ts",
        r#"
        export default defineConfig({
          build: { rollupOptions: { input: 'src/main.ts' } },
          test: { coverage: { exclude: ['src/legacy.ts'] } },
        })
        "#,
    );
    let universe = paths(&[
        "vitest.config.ts",
        "src/main.ts",
        "src/legacy.ts",
        "src/nobody-names-me.ts",
    ]);

    let found = scan(t.path(), &universe);

    assert!(found.contains("src/main.ts"), "the entry: {found:?}");
    assert!(
        found.contains("src/legacy.ts"),
        "KNOWN COST: an exclusion path is harvested too, and this pin is what keeps that visible \
         instead of silent: {found:?}"
    );
    assert!(
        !found.contains("src/nobody-names-me.ts"),
        "the harvest must still reach nothing no config spells: {found:?}"
    );
}

/// **The other side of not reading the key**: an entry declared WITHOUT its extension — legal in
/// rollup and vite, which resolve it themselves — is missed, because the extension is the fence that
/// keeps an alias table (`a_directory_string_with_no_extension_resolves_to_nothing` above) from
/// excusing a whole source tree. Two files, one named with its extension and one without, in the same
/// call: the pair is the disclosure, and it is what makes the miss a stated trade rather than a bug
/// nobody wrote down.
#[test]
fn an_entry_declared_without_its_extension_is_not_exempted() {
    let t = TempTree::new();
    t.write(
        "rollup.config.js",
        "export default { input: ['src/with-ext.ts', 'src/no-ext'] }",
    );
    let universe = paths(&["rollup.config.js", "src/with-ext.ts", "src/no-ext.ts"]);
    assert_eq!(
        scan(t.path(), &universe),
        paths(&["src/with-ext.ts"]),
        "src/no-ext.ts stays a candidate — the extension requirement is the alias fence"
    );
}

/// The extension match is case-INSENSITIVE because the rule's own eligibility test
/// (`dead_candidates::is_ts_dispatch_extension`) is `(?i)`. A case-sensitive harvest would have left
/// an oddly-cased file eligible for the finding and invisible to the exemption — the two predicates
/// are hand copies across a deliberate crate boundary, so only a test can hold them together.
#[test]
fn the_extension_match_follows_the_rules_own_case_insensitivity() {
    let t = TempTree::new();
    t.write(
        "vite.config.ts",
        "export default { entry: './src/Main.TS' }",
    );
    let universe = paths(&["vite.config.ts", "src/Main.TS"]);
    assert_eq!(scan(t.path(), &universe), paths(&["src/Main.TS"]));
}

/// A config in the universe with no file behind it (deleted between the walk and this pass, or
/// unreadable) degrades to contributing nothing rather than aborting the run — and the OTHER config in
/// the same tree still contributes, so the degradation is per file and not per tree.
#[test]
fn an_unreadable_config_degrades_without_taking_the_others_with_it() {
    let t = TempTree::new();
    t.write(
        "vite.config.ts",
        "export default { entry: './src/main.ts' }",
    );
    let universe = paths(&["vite.config.ts", "never-written.config.ts", "src/main.ts"]);
    assert_eq!(scan(t.path(), &universe), paths(&["src/main.ts"]));
}

/// **A file a tool OWNS is not a file that DECLARES**, and for one review cycle this pass did not tell
/// them apart. The dot-directory exemption widened from the `config` stem to every file at depth 1 —
/// correct for the exemption, since Storybook writes `main.mjs` — and because the harvest read the same
/// predicate, an ordinary string inside `.storybook/copyAssets.ts` became an entry declaration.
///
/// This is not hypothetical: `corpus/frameworks/grafana/packages/grafana-ui/.storybook/copyAssets.ts`
/// names `'../src/components/Icon/utils.ts'`, and a source file with no importer went silent on the
/// strength of a guess about who owns a directory — `rule-quality.md` §24's erasing direction with no
/// declaration under it. The predicates were split; this pin is what keeps them split.
#[test]
fn a_tool_owned_file_that_is_not_a_config_is_never_read_as_a_declaration_source() {
    let t = TempTree::new();
    // A real declaration source, and a tool-owned script that merely mentions a path.
    t.write(
        ".storybook/config.js",
        "export default { entry: '../src/story-entry.ts' };",
    );
    t.write(
        ".storybook/copyAssets.ts",
        "export const assets = ['../src/orphan.ts'];",
    );
    let universe = paths(&[
        ".storybook/config.js",
        ".storybook/copyAssets.ts",
        "src/story-entry.ts",
        "src/orphan.ts",
    ]);

    let found = scan(t.path(), &universe);

    assert_eq!(
        found,
        paths(&["src/story-entry.ts"]),
        "only the `config` stem declares; a tool-owned script's string must mint nothing: {found:?}"
    );
}
