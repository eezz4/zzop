//! End-to-end tests for the ESM side-effect (specifier-less) import — `import "./x";` — as a real
//! dep-graph edge (`zzop_parser_typescript::parse_imports`, projected by
//! `lang::resolve::dep_graph::build_dep_impl`, consumed at `analyze::assemble::dep_graph`).
//!
//! `import "./x";` loads the module and runs its top-level effects, which is exactly how registry-style
//! codebases wire pages/config/side-effect modules (measured on getredash/redash: a `pages/index.js`
//! whose entire body is 32 such statements). The parser dropped it — its `for spec in &import.specifiers`
//! loop simply had nothing to iterate — so every such target had zero importers and false-fired
//! `dead-candidates` and `unreachable`.
//!
//! This change makes the graph LARGER, so its only possible effect on findings is SUPPRESSION, and a
//! suppression fails silently. Every test here therefore carries a never-over-suppress control in the
//! SAME fixture: a module nobody references at all must stay flagged, and a file reached only by a
//! side-effect import must keep reporting its exports as `unimported-export` (a side-effect import
//! consumes no NAMED export, so treating it as a wildcard use would blank that rule out).

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

fn files_for(out: &zzop_engine::AnalyzeOutput, rule: &str) -> Vec<String> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == rule)
        .map(|f| f.file.clone())
        .collect()
}

/// The reproduction, end to end. `src/index.js` loads `./Leaf` for its effects only and `./Named` by
/// name. Before the fix `src/Leaf.js` was a `dead-candidates` finding ("no importers found in this
/// tree") while `src/Named.js` was correctly silent.
///
/// Controls in the same fixture, both required for this to be readable as anything other than a blanket
/// suppression:
/// - `src/Orphan.js` is referenced by nobody — it MUST stay a dead-candidate.
/// - `leaf` is exported by `src/Leaf.js` and imported by NAME nowhere, so `unimported-export` must still
///   fire on it. That is the pin against giving the synthetic binding a `"*"` original, which
///   `find_dead_exports` reads as "every export of the target is used".
#[test]
fn side_effect_imported_module_is_not_dead_but_orphan_and_its_unused_export_stay() {
    let dir = TempDir::new("zzop-engine-sideeffect-import");
    dir.write(
        "src/index.js",
        "import \"./Leaf\";\nimport { named } from \"./Named\";\nnamed();\n",
    );
    dir.write(
        "src/Leaf.js",
        "export function leaf() { return 1; }\nleaf();\n",
    );
    dir.write("src/Named.js", "export function named() { return 2; }\n");
    dir.write("src/Orphan.js", "export function orphan() { return 3; }\n");

    let out = analyze_tree(dir.path(), &config());
    let dead = files_for(&out, "dead-candidates");
    let unimported = files_for(&out, "unimported-export");

    assert!(
        !dead.contains(&"src/Leaf.js".to_string()),
        "src/Leaf.js is loaded by `import \"./Leaf\";` and must NOT be a dead-candidate, got dead: {dead:?}"
    );
    assert!(
        !dead.contains(&"src/Named.js".to_string()),
        "the named-import control must stay silent, got dead: {dead:?}"
    );
    assert!(
        dead.contains(&"src/Orphan.js".to_string()),
        "a module nobody references MUST stay a dead-candidate — without this the fix is \
         indistinguishable from disabling the rule. got dead: {dead:?}"
    );
    assert!(
        unimported.contains(&"src/Leaf.js".to_string()),
        "a side-effect import consumes no NAMED export, so `leaf` must still be an unimported-export — \
         a `\"*\"` original here would blank the rule out for the whole file. got: {unimported:?}"
    );
}

/// The `unreachable` half of the same defect: a registry module (`src/pages/index.js`) whose entire body
/// is side-effect imports, reached from the entry. Its targets used to be `unreachable` islands because
/// the registry had no out-edges at all.
///
/// Two never-over-suppress controls in the same fixture, one per rule this test moves:
/// - `src/pages/detached.js` is a page the registry does NOT list and nothing else imports (fanIn 0), so
///   it must stay a dead-candidate.
/// - `src/island/{a,b}.js` import each OTHER by name — a closed island with fanIn > 0 that no entry
///   reaches, built out of ordinary named imports so the control is independent of this change. It must
///   stay `unreachable`; without it, `unreachable: []` would read exactly like a clean run.
///
/// (`detached.js` is NOT an `unreachable` candidate: that rule takes every fanIn=0 file as an entrypoint
/// itself, false-positive-safe, and reports it through `dead-candidates` instead — the two rules never
/// flag the same file.)
#[test]
fn side_effect_registry_reaches_its_pages_and_a_closed_island_stays_unreachable() {
    let dir = TempDir::new("zzop-engine-sideeffect-registry");
    dir.write("src/index.js", "import './pages';\n");
    dir.write(
        "src/pages/index.js",
        "import './home';\nimport './about';\n",
    );
    dir.write("src/pages/home.js", "console.log('home page');\n");
    dir.write("src/pages/about.js", "console.log('about page');\n");
    // Not listed by the registry — nothing loads it.
    dir.write("src/pages/detached.js", "console.log('detached page');\n");
    // Closed named-import island: fanIn > 0 on both, reachable from no entry.
    dir.write(
        "src/island/a.js",
        "import { b } from './b';\nexport const a = () => b();\n",
    );
    dir.write(
        "src/island/b.js",
        "import { a } from './a';\nexport const b = () => a;\n",
    );

    let out = analyze_tree(dir.path(), &config());
    let dead = files_for(&out, "dead-candidates");
    let unreach = files_for(&out, "unreachable");

    for live in [
        "src/pages/index.js",
        "src/pages/home.js",
        "src/pages/about.js",
    ] {
        assert!(
            !dead.contains(&live.to_string()),
            "{live} is reached through a side-effect import chain and must NOT be a dead-candidate, got dead: {dead:?}"
        );
        assert!(
            !unreach.contains(&live.to_string()),
            "{live} is reached from the entry through side-effect imports and must NOT be unreachable, got unreachable: {unreach:?}"
        );
    }
    assert!(
        dead.contains(&"src/pages/detached.js".to_string()),
        "a page the registry does not list MUST stay a dead-candidate, got dead: {dead:?}"
    );
    for stranded in ["src/island/a.js", "src/island/b.js"] {
        assert!(
            unreach.contains(&stranded.to_string()),
            "the closed named-import island MUST stay unreachable — a rule reporting 0 here is \
             indistinguishable from a rule that stopped running. got unreachable: {unreach:?}"
        );
    }
}

/// The CommonJS counterpart was already handled (`__require{N}__`), and it must stay handled: this pins
/// that the ESM change did not disturb the require walk, and that the two synthetic key namespaces
/// coexist in one file without one displacing the other.
#[test]
fn bare_require_and_bare_import_in_one_file_both_produce_edges() {
    let dir = TempDir::new("zzop-engine-sideeffect-mixed");
    dir.write("src/index.js", "import './esm';\nrequire('./cjs');\n");
    dir.write("src/esm.js", "console.log('esm side effect');\n");
    dir.write("src/cjs.js", "console.log('cjs side effect');\n");
    dir.write("src/stale.js", "console.log('nobody loads me');\n");

    let out = analyze_tree(dir.path(), &config());
    let dead = files_for(&out, "dead-candidates");

    for live in ["src/esm.js", "src/cjs.js"] {
        assert!(
            !dead.contains(&live.to_string()),
            "{live} is side-effect loaded and must NOT be a dead-candidate, got dead: {dead:?}"
        );
    }
    assert!(
        dead.contains(&"src/stale.js".to_string()),
        "an unloaded sibling MUST stay a dead-candidate, got dead: {dead:?}"
    );
}
