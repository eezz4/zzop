//! End-to-end tests for runtime asset-URL reference reachability
//! (`zzop_parser_typescript::parse_asset_refs`, wired at `analyze::assemble::dep_graph::
//! merge_asset_ref_fan_in` + the `unreachable` `extra_entries` seed at `analyze::assemble::rules`).
//!
//! A `public/`-served `.js` worklet/worker loaded only via a runtime URL STRING
//! (`audioWorklet.addModule("/…")`, `new Worker`, `importScripts`, `new URL(_, import.meta.url)`) is
//! invisible to the static import graph, so before this pass it false-fired `dead-candidates` (the
//! mono-hub `rnnoiseWorklet.js` field FP this fix closes). These tests exercise the whole real pipeline
//! (`analyze_tree`, real source on disk) end to end, plus the two safety pins the design meeting called
//! out: the served-path→`public/` resolution must NOT revive a differently-named unreferenced sibling
//! (false-negative bound), and the bumped target must NOT flip into a false `unreachable` island (the
//! mandatory seed). A genuinely-dead build script and an unreferenced public sibling are the
//! never-over-suppress controls.

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
    out.findings
        .iter()
        .filter(|f| f.rule_id == "dead-candidates")
        .map(|f| f.file.clone())
        .collect()
}

fn unreachable(out: &zzop_engine::AnalyzeOutput) -> Vec<String> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == "unreachable")
        .map(|f| f.file.clone())
        .collect()
}

/// The served-absolute `public/` case (the mono-hub FP shape): a `.ts` module reachable from an entry
/// loads a `public/`-served worklet by `audioWorklet.addModule("/…")`. The worklet must drop out of
/// `dead-candidates` AND not become a false `unreachable` island; a differently-named unreferenced public
/// sibling and a genuinely-dead build script must both STAY flagged (never-over-suppress controls).
#[test]
fn public_worklet_loaded_by_add_module_is_not_dead_but_orphan_and_script_stay() {
    let dir = TempDir::new("zzop-engine-assetref-public");
    dir.write(
        "src/main.ts",
        "import { apply } from './features/applyRnnoise';\napply();\n",
    );
    dir.write(
        "src/features/applyRnnoise.ts",
        "export function apply(ctx: any) {\n  ctx.audioWorklet.addModule(\"/noise-suppressor/rnnoiseWorklet.js\");\n}\n",
    );
    // The referenced worklet — reachable ONLY via the addModule URL string.
    dir.write(
        "public/noise-suppressor/rnnoiseWorklet.js",
        "// vendor worklet\nregisterProcessor('rnnoise', class {});\n",
    );
    // A differently-named, unreferenced public sibling — the false-negative-bound control: nothing loads
    // it, so it MUST stay a dead-candidate (the suffix resolver must not revive it).
    dir.write(
        "public/noise-suppressor/orphan.js",
        "// nobody loads this\nexport const z = 1;\n",
    );
    // A genuinely-dead build script (the mono-hub `scripts/*.cjs` baseline) — MUST stay flagged.
    dir.write("scripts/build.cjs", "console.log('build');\n");

    let out = analyze_tree(dir.path(), &config());
    let dead = dead_candidates(&out);
    let unreach = unreachable(&out);

    assert!(
        !dead.contains(&"public/noise-suppressor/rnnoiseWorklet.js".to_string()),
        "the addModule-loaded worklet must NOT be a dead-candidate, got dead: {dead:?}"
    );
    assert!(
        !unreach.contains(&"public/noise-suppressor/rnnoiseWorklet.js".to_string()),
        "the addModule-loaded worklet must NOT be a false unreachable island (seed), got unreachable: {unreach:?}"
    );
    assert!(
        dead.contains(&"public/noise-suppressor/orphan.js".to_string()),
        "an unreferenced public sibling must STAY a dead-candidate (false-negative bound), got dead: {dead:?}"
    );
    assert!(
        dead.contains(&"scripts/build.cjs".to_string()),
        "a genuinely-dead build script must STAY a dead-candidate, got dead: {dead:?}"
    );
}

/// The canonical bundler worker idiom with the `new URL(...)` NESTED as `new Worker`'s first argument
/// (`new Worker(new URL("./worker.ts", import.meta.url), { type: "module" })`) — the shape the backlog FP
/// report named. `new Worker`'s own first-arg capture sees a non-literal here, so the reference is only
/// visible through the nested `new URL` visit. Both the module-relative worker and a `../`-relative
/// `SharedWorker` must drop out of `dead-candidates` without becoming false `unreachable` islands, while a
/// sibling module nobody references at all MUST stay flagged (never-over-suppress control).
#[test]
fn nested_new_worker_new_url_is_not_dead_but_unreferenced_sibling_stays() {
    let dir = TempDir::new("zzop-engine-assetref-nested");
    dir.write("src/main.ts", "import { boot } from './boot';\nboot();\n");
    dir.write(
        "src/boot.ts",
        "export function boot() {\n  \
         const w = new Worker(new URL(\"./worker.ts\", import.meta.url), { type: \"module\" });\n  \
         const s = new SharedWorker(new URL(\"../shared/bus.ts\", import.meta.url));\n  \
         return [w, s];\n}\n",
    );
    dir.write("src/worker.ts", "self.onmessage = () => {};\nexport {};\n");
    dir.write("shared/bus.ts", "export const bus = 1;\n");
    // Nothing references this at all — it MUST stay a dead-candidate, or the fix is a blanket suppression.
    dir.write("src/stale.ts", "export const stale = 1;\n");

    let out = analyze_tree(dir.path(), &config());
    let dead = dead_candidates(&out);
    let unreach = unreachable(&out);

    for live in ["src/worker.ts", "shared/bus.ts"] {
        assert!(
            !dead.contains(&live.to_string()),
            "{live} is referenced by a nested new Worker(new URL(_, import.meta.url)) and must NOT be a dead-candidate, got dead: {dead:?}"
        );
        assert!(
            !unreach.contains(&live.to_string()),
            "{live} must NOT be a false unreachable island (extra_entries seed), got unreachable: {unreach:?}"
        );
    }
    assert!(
        dead.contains(&"src/stale.ts".to_string()),
        "an unreferenced module must STAY a dead-candidate, got dead: {dead:?}"
    );
}

/// The OTHER half of the bundler worker idiom: the constructor import
/// (`import MyWorker from "./worker?worker"`, Vite's documented form). The `?worker` resource query is a
/// bundler instruction, not part of the filename, so the specifier must resolve to `src/worker.ts` like
/// any other relative import — otherwise the worker entry has no importer and false-fires
/// `dead-candidates`. `?url`/`?raw` (asset queries) resolve the same way; an unreferenced sibling must
/// still be flagged.
#[test]
fn resource_query_import_resolves_the_worker_entry() {
    let dir = TempDir::new("zzop-engine-assetref-query");
    dir.write(
        "src/main.ts",
        "import MyWorker from './worker?worker';\nimport Shared from './bus?sharedworker';\nimport css from './style.ts?url';\nnew MyWorker();\nnew Shared();\nconsole.log(css);\n",
    );
    dir.write("src/worker.ts", "self.onmessage = () => {};\nexport {};\n");
    dir.write("src/bus.ts", "export const bus = 1;\n");
    dir.write("src/style.ts", "export const style = 1;\n");
    dir.write("src/stale.ts", "export const stale = 1;\n");

    let out = analyze_tree(dir.path(), &config());
    let dead = dead_candidates(&out);
    let unreach = unreachable(&out);

    for live in ["src/worker.ts", "src/bus.ts", "src/style.ts"] {
        assert!(
            !dead.contains(&live.to_string()),
            "{live} is imported with a bundler resource query and must NOT be a dead-candidate, got dead: {dead:?}"
        );
        assert!(
            !unreach.contains(&live.to_string()),
            "{live} must NOT be unreachable — it is imported by the entry, got unreachable: {unreach:?}"
        );
    }
    assert!(
        dead.contains(&"src/stale.ts".to_string()),
        "an unreferenced module must STAY a dead-candidate, got dead: {dead:?}"
    );
}

/// Never-guess bound for the worker shape: a COMPUTED worker URL
/// (`new Worker(new URL(name, import.meta.url))`) yields no reference, so a module that merely LOOKS like
/// it could be that worker is still reported dead. Guessing here would suppress real dead code tree-wide.
#[test]
fn computed_worker_url_does_not_revive_a_candidate() {
    let dir = TempDir::new("zzop-engine-assetref-computed");
    dir.write("src/main.ts", "import { boot } from './boot';\nboot();\n");
    dir.write(
        "src/boot.ts",
        "export function boot(name: string) {\n  \
         return new Worker(new URL(name, import.meta.url), { type: \"module\" });\n}\n",
    );
    dir.write("src/worker.ts", "self.onmessage = () => {};\nexport {};\n");

    let out = analyze_tree(dir.path(), &config());
    let dead = dead_candidates(&out);
    assert!(
        dead.contains(&"src/worker.ts".to_string()),
        "a computed worker URL must not revive anything (never-guess), got dead: {dead:?}"
    );
}

/// The relative-resolution branch: `new URL("./worker.ts", import.meta.url)` (the Vite worker/asset
/// idiom) resolves the sibling like a normal module import. The worker file, referenced ONLY this way,
/// must drop out of `dead-candidates` and not become a false `unreachable` island.
#[test]
fn relative_new_url_worker_is_not_dead() {
    let dir = TempDir::new("zzop-engine-assetref-relative");
    dir.write("src/main.ts", "import { boot } from './boot';\nboot();\n");
    dir.write(
        "src/boot.ts",
        "export function boot() {\n  const u = new URL(\"./worker.ts\", import.meta.url);\n  new Worker(u);\n}\n",
    );
    dir.write("src/worker.ts", "self.onmessage = () => {};\nexport {};\n");

    let out = analyze_tree(dir.path(), &config());
    let dead = dead_candidates(&out);
    let unreach = unreachable(&out);

    assert!(
        !dead.contains(&"src/worker.ts".to_string()),
        "a worker referenced via new URL(_, import.meta.url) must NOT be a dead-candidate, got dead: {dead:?}"
    );
    assert!(
        !unreach.contains(&"src/worker.ts".to_string()),
        "the new-URL worker must NOT be a false unreachable island, got unreachable: {unreach:?}"
    );
}
