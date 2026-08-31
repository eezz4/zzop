//! End-to-end test for the tool-config entry harvest
//! (`analyze::assemble::rules::config_entries`, unioned into `dead-candidates`' `extra_entries` at
//! `analyze::assemble::rules`).
//!
//! `dead-candidates` already exempted a config FILE from candidacy; it did not read the file. A
//! bundler config is where a build states which files it starts from, so discarding that text turned
//! every application entry into an orphan. Measured on koel 3f5213d4: 6 of 7 findings were wrong, and
//! three of them — `resources/assets/js/app.ts`, `resources/assets/js/remote/app.ts`,
//! `resources/assets/js/service-worker.ts` — were named in plain text inside two config files zzop had
//! already opened its exemption for.
//!
//! These run the whole real pipeline against real source on disk, and every one of them asserts the
//! never-over-suppress control in the SAME call: a file no config names must stay flagged. A pass that
//! only proved the silences would also pass if the harvest had excused the tree.

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

/// The koel shape end to end: a root config declaring two entries as a positional argument to a plugin
/// CALL, and a SECOND config for the same tool (`vite.config.sw.js`, reached only through the tool's
/// `--config` flag) declaring a third through `build.lib.entry`. All three must drop out; a module no
/// config names must stay in.
#[test]
fn entries_declared_by_two_vite_configs_drop_out_while_an_unnamed_module_stays() {
    let dir = TempDir::new("zzop-engine-config-entries");
    dir.write(
        "vite.config.ts",
        "import laravel from 'laravel-vite-plugin';\n\
         import { resolve } from 'path';\n\
         export default {\n  \
           plugins: [laravel({ input: ['resources/assets/js/app.ts', 'resources/assets/js/remote/app.ts'] })],\n  \
           resolve: { alias: { '@': resolve(__dirname, './resources/assets/js') } },\n\
         };\n",
    );
    dir.write(
        "vite.config.sw.js",
        "import { resolve } from 'path';\n\
         export default {\n  \
           build: { lib: { entry: resolve(__dirname, 'resources/assets/js/service-worker.ts') }, outDir: 'public' },\n\
         };\n",
    );
    dir.write("resources/assets/js/app.ts", "export const boot = 1;\n");
    dir.write(
        "resources/assets/js/remote/app.ts",
        "export const remote = 1;\n",
    );
    dir.write(
        "resources/assets/js/service-worker.ts",
        "self.oninstall = () => {};\nexport {};\n",
    );
    // Named by NO config and imported by nothing — the never-over-suppress control. This is the koel
    // file two independent auditors judged genuinely dead and deleted.
    dir.write(
        "resources/assets/js/config/acceptedImageTypes.ts",
        "export const acceptedImageTypes = ['image/png'];\n",
    );

    let dead = dead_candidates(&analyze_tree(dir.path(), &config()));

    for entry in [
        "resources/assets/js/app.ts",
        "resources/assets/js/remote/app.ts",
        "resources/assets/js/service-worker.ts",
    ] {
        assert!(
            !dead.contains(&entry.to_string()),
            "a config-declared entry must not be a dead candidate: {entry} in {dead:?}"
        );
    }
    assert!(
        dead.contains(&"resources/assets/js/config/acceptedImageTypes.ts".to_string()),
        "a module no config names must STAY a dead candidate, got: {dead:?}"
    );
}

/// The alias table is why an entry must carry an EXTENSION. `'@': resolve(__dirname, './src')` names
/// the application's whole source root; if a directory string could resolve, that one line would
/// excuse every file beneath it. The dead module here sits inside exactly that directory.
#[test]
fn an_alias_naming_a_source_directory_does_not_excuse_the_files_under_it() {
    let dir = TempDir::new("zzop-engine-config-alias");
    dir.write(
        "vite.config.ts",
        "import { resolve } from 'path';\n\
         export default { resolve: { alias: { '@': resolve(__dirname, './src') } } };\n",
    );
    dir.write("src/main.ts", "export const main = 1;\n");
    dir.write("src/orphan.ts", "export const orphan = 1;\n");

    let dead = dead_candidates(&analyze_tree(dir.path(), &config()));
    assert!(
        dead.contains(&"src/orphan.ts".to_string()),
        "a file under an aliased directory must STAY a dead candidate, got: {dead:?}"
    );
}

/// A VitePress config sits at a fixed convention path (`.vitepress/config.*`) that the
/// `<name>.config.<ext>` shape cannot see, so it was reported as a dead file itself. It is exempt now
/// through the dot-directory shape — and a sibling module inside the SAME dot-directory is not, since
/// the shape admits the `config` stem and not the directory.
#[test]
fn a_dot_directory_config_is_exempt_while_its_sibling_module_is_not() {
    let dir = TempDir::new("zzop-engine-config-dotdir");
    dir.write(
        "docs/.vitepress/config.mts",
        "export default { title: 'docs', themeConfig: { nav: [] } };\n",
    );
    dir.write("docs/.vitepress/main.mjs", "export default { x: 1 };\n");
    dir.write(
        "docs/.vitepress/theme/orphan.ts",
        "export const orphan = 1;\n",
    );

    let dead = dead_candidates(&analyze_tree(dir.path(), &config()));
    assert!(
        !dead.contains(&"docs/.vitepress/config.mts".to_string()),
        "a dot-directory tool config must not be a dead candidate, got: {dead:?}"
    );
    // The boundary is DEPTH, not the stem — this pin asserted the stem until 2026-08-21, when
    // apache/superset showed `.storybook/main.mjs` reported as dead. Storybook writes `main`,
    // `preview` and `manager`, so a `config`-only rule was a guess about filenames wearing the
    // clothes of a rule about directories. Note the doc's own negative example was always a
    // depth-TWO path, which is what the widened shape still excludes.
    assert!(
        !dead.contains(&"docs/.vitepress/main.mjs".to_string()),
        "a file directly inside a dot-directory is tool-owned whatever its stem, got: {dead:?}"
    );
    assert!(
        dead.contains(&"docs/.vitepress/theme/orphan.ts".to_string()),
        "the exemption is depth ONE — a nested module is still ordinary source, got: {dead:?}"
    );
}
