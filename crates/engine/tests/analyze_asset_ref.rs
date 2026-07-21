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
