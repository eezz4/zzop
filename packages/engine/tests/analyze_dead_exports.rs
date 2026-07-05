//! End-to-end tests for the `dead-exports` native analysis (`zpz_engine::dead_exports` wiring around
//! `zpz_rules_graph::find_dead_exports`) — the symbol-level companion to the file-level `dead-candidates` analysis.
//! Unlike `rules/native/rules-graph/src/dead_exports.rs`'s unit tests (hand-built `DeadExportInputFile`s, no parsing
//! involved), these tests exercise the whole real pipeline — real TS source on disk, through
//! `analyze_tree`, through the fused per-file pass (`used_names` collection) and the second uncached
//! re-exports/dynamic-imports pass — to prove the wiring itself (cache field plumbing, native-analysis
//! gating, symbol-line lookup, entry-file exemption) actually connects end to end, not just that the pure
//! algorithm is correct in isolation.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zpz_engine::{analyze_tree, EngineConfig};

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

#[test]
fn unused_export_produces_a_dead_exports_finding_at_its_declaration_line() {
    let dir = TempDir::new("zpz-engine-dead-exports-unused");
    dir.write(
        "lib/util.ts",
        "export function used() { return 1; }\nexport function unused() { return 2; }\n",
    );
    dir.write(
        "consumer.ts",
        "import { used } from './lib/util';\nexport const x = used();\n",
    );
    let out = analyze_tree(dir.path(), &config());
    let hit = out
        .findings
        .iter()
        .find(|f| f.rule_id == "dead-exports" && f.file == "lib/util.ts");
    assert!(
        hit.is_some(),
        "expected a dead-exports finding for lib/util.ts, got: {:?}",
        out.findings
    );
    let hit = hit.unwrap();
    assert_eq!(hit.line, 2); // `unused`'s declaration line
    assert!(hit.message.contains("unused"));
    // The imported `used` export must NOT be flagged.
    assert!(!out.findings.iter().any(
        |f| f.rule_id == "dead-exports" && f.data.as_ref().is_some_and(|d| d["name"] == "used")
    ));
}

#[test]
fn entry_index_file_exports_are_exempt_even_when_unimported() {
    let dir = TempDir::new("zpz-engine-dead-exports-entry");
    // `index.ts` is an entry/barrel file per zpz's ENTRY_PATTERNS — its exports are public API surface and
    // must never be flagged, even with zero in-repo importers.
    dir.write(
        "src/index.ts",
        "export function publicApi() { return 1; }\n",
    );
    let out = analyze_tree(dir.path(), &config());
    assert!(!out.findings.iter().any(|f| f.rule_id == "dead-exports"));
}

#[test]
fn barrel_re_export_from_an_entry_file_is_a_live_root() {
    let dir = TempDir::new("zpz-engine-dead-exports-barrel");
    dir.write("src/impl.ts", "export function impl() { return 1; }\n");
    // `index.ts` (an entry file) re-exports `impl` — no in-repo consumer imports it through the barrel, but
    // it is public API surface (zpz: "entry re-export is a live root even with no consumer").
    dir.write("src/index.ts", "export { impl } from './impl';\n");
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-exports" && f.file == "src/impl.ts"),
        "impl should be a live root via the entry re-export, got: {:?}",
        out.findings
    );
}

#[test]
fn export_referenced_only_within_its_own_file_is_flagged_in_file_only() {
    let dir = TempDir::new("zpz-engine-dead-exports-in-file-only");
    dir.write(
        "lib/helper.ts",
        "export const HELPER = 1;\nexport function use() { return HELPER; }\n",
    );
    let out = analyze_tree(dir.path(), &config());
    let hit = out.findings.iter().find(|f| {
        f.rule_id == "dead-exports" && f.data.as_ref().is_some_and(|d| d["name"] == "HELPER")
    });
    assert!(
        hit.is_some(),
        "expected an in-file-only dead-exports finding for HELPER, got: {:?}",
        out.findings
    );
    let reason = hit.unwrap().data.as_ref().unwrap()["reason"].clone();
    assert_eq!(reason, "in-file-only");
}

#[test]
fn disabling_dead_exports_removes_the_finding() {
    let dir = TempDir::new("zpz-engine-dead-exports-disabled");
    dir.write("lib/orphan.ts", "export function orphan() { return 1; }\n");
    let mut cfg = config();
    cfg.rule_config
        .disabled_rules
        .push("dead-exports".to_string());
    let out = analyze_tree(dir.path(), &cfg);
    assert!(!out.findings.iter().any(|f| f.rule_id == "dead-exports"));
}
