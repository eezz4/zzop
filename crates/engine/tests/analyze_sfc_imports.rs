//! End-to-end tests for the `.vue`/`.svelte` SFC `<script>`-block import pre-scan
//! (`zzop_parser_typescript::extract_sfc_script_imports`, wired at `analyze::assemble::sfc` +
//! `analyze::assemble::dep_graph::merge_sfc_fan_in` + `dead_exports::dead_export_findings`'s
//! `sfc_import_pairs` parameter).
//!
//! `.vue`/`.svelte` files dispatch to `None` (no structural parser frontend — `zzop_engine::dispatch`),
//! so before this pre-scan a `.ts` symbol imported and used ONLY inside a component's `<script>` block had
//! zero visible fan-in through the normal fused pipeline, false-firing `dead-exports`/`dead-candidates`.
//! These tests exercise the whole real pipeline (`analyze_tree`, real `.vue` source on disk) to prove the
//! wiring connects end to end, plus the two safety pins the parser-owner review called out: (F3) the
//! `.vue`/`.svelte` file itself must never become a NEW `dead-candidates` false positive (it must never
//! mint its own dep-graph node), and (never-over-suppress) a genuinely dead `.ts` export must still be
//! flagged even in a tree that also has a live SFC-only consumer.

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

/// `src/composable/use-x.ts` exports `useX` (consumed ONLY by `src/App.vue`'s `<script setup>` block) and
/// `unusedHelper` (consumed by nobody at all — the never-over-suppress regression control).
fn write_vue_fixture(dir: &TempDir) {
    dir.write(
        "src/composable/use-x.ts",
        "export function useX() { return 1; }\nexport function unusedHelper() { return 2; }\n",
    );
    dir.write(
        "src/App.vue",
        "<script setup>\nimport { useX } from './composable/use-x';\nuseX();\n</script>\n<template>\n  <div/>\n</template>\n",
    );
}

#[test]
fn vue_script_setup_import_clears_dead_exports_on_the_consumed_symbol() {
    let dir = TempDir::new("zzop-engine-sfc-dead-exports");
    write_vue_fixture(&dir);
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.findings.iter().any(|f| f.rule_id == "dead-exports"
            && f.file == "src/composable/use-x.ts"
            && f.data.as_ref().is_some_and(|d| d["name"] == "useX")),
        "useX should not be dead-exports once the .vue <script>-block pre-scan sees the import, got: {:?}",
        out.findings
    );

    // Regression control (never-over-suppress pin): `unusedHelper` has no consumer anywhere (not even the
    // .vue file) and must still be flagged — the pre-scan must not blanket-suppress every export in a
    // file an SFC happens to import from.
    assert!(
        out.findings.iter().any(|f| f.rule_id == "dead-exports"
            && f.file == "src/composable/use-x.ts"
            && f.data.as_ref().is_some_and(|d| d["name"] == "unusedHelper")),
        "unusedHelper (no consumer) should still be flagged dead-exports, got: {:?}",
        out.findings
    );
}

#[test]
fn vue_script_setup_import_clears_dead_candidates_on_the_consumed_file() {
    let dir = TempDir::new("zzop-engine-sfc-dead-candidates");
    write_vue_fixture(&dir);
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/composable/use-x.ts"),
        "use-x.ts should not be dead-candidates once the .vue <script>-block pre-scan gives it fan-in, \
         got: {:?}",
        out.findings
    );
}

/// The F3 pin (parser-owner review): the `.vue`/`.svelte` file itself must never become a NEW
/// `dead-candidates` false positive as a side effect of this pre-scan — the source-only injection
/// (`dep_graph::merge_sfc_fan_in`) must never mint a dep-graph node for it. Without this pin, `App.vue`
/// would have picked up an outgoing edge to `use-x.ts` with zero incoming edges of its own ("mounted by
/// convention", the same shape every other framework entry file is exempted from).
#[test]
fn vue_file_itself_never_becomes_a_new_dead_candidate() {
    let dir = TempDir::new("zzop-engine-sfc-f3-pin");
    write_vue_fixture(&dir);
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/App.vue"),
        "App.vue must never itself become a dead-candidates false positive (F3 pin), got: {:?}",
        out.findings
    );
    // Same pin, from the dep-graph's own side: `App.vue` must never appear as a dep-graph key/target at
    // all — a source-only injection, per `merge_sfc_fan_in`'s own doc.
    assert!(
        !out.ir.ir.dep.contains_key("src/App.vue"),
        "App.vue must never mint its own dep-graph entry, got keys: {:?}",
        out.ir.ir.dep.keys().collect::<Vec<_>>()
    );
}

/// A `.ts` imported ONLY by an SFC `<script>` block gets real fan-in (so it is not a `dead-candidates`
/// FP) but sits behind no `dep` edge from any entry (the SFC is not a graph node), so it — and anything it
/// transitively imports — would read as a false `unreachable` island. The SFC target is seeded into
/// `unreachable`'s `extra_entries` (a framework-mounted component is effectively an entrypoint), so neither
/// the directly-imported composable nor its transitive import is flagged.
#[test]
fn ts_imported_only_by_an_sfc_is_not_a_false_unreachable_island() {
    let dir = TempDir::new("zzop-engine-sfc-unreachable");
    dir.write(
        "src/composable/use-y.ts",
        "export function useY() { return 2; }\n",
    );
    dir.write(
        "src/composable/use-x.ts",
        "import { useY } from './use-y';\nexport function useX() { return useY(); }\n",
    );
    dir.write(
        "src/App.vue",
        "<script setup>\nimport { useX } from './composable/use-x';\nuseX();\n</script>\n<template><div/></template>\n",
    );
    let out = analyze_tree(dir.path(), &config());

    let unreachable: Vec<&str> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "unreachable")
        .map(|f| f.file.as_str())
        .collect();
    assert!(
        !unreachable.contains(&"src/composable/use-x.ts")
            && !unreachable.contains(&"src/composable/use-y.ts"),
        "an SFC-imported .ts (and its transitive import) must not be a false unreachable island, got: {:?}",
        out.findings
    );
}

/// A `.vue` file with no `<script>` block at all (or a `<script>` with no imports) contributes nothing —
/// the pre-scan must not fabricate liveness signal that isn't there.
#[test]
fn vue_file_with_no_script_import_leaves_dead_exports_untouched() {
    let dir = TempDir::new("zzop-engine-sfc-no-script-import");
    dir.write(
        "src/composable/use-y.ts",
        "export function useY() { return 1; }\n",
    );
    dir.write(
        "src/App.vue",
        "<template>\n  <div>hi</div>\n</template>\n<script setup>\nconst greeting = 'hi';\n</script>\n",
    );
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.findings.iter().any(|f| f.rule_id == "dead-exports"
            && f.file == "src/composable/use-y.ts"
            && f.data.as_ref().is_some_and(|d| d["name"] == "useY")),
        "useY (no consumer anywhere, including the script-import-less .vue) should still be flagged \
         dead-exports, got: {:?}",
        out.findings
    );
}
