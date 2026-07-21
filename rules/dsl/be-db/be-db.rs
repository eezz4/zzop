//! End-to-end tests for `rules/dsl/be-db/be-db.json`, exercised via `zzop_engine::analyze_tree` so
//! `Matcher::MethodScan` rules run against real parser-derived `SourceSymbol` body spans (not hand-built
//! spans). See `be-db.json` for each rule's exact trigger/veto shape and message.
//!
//! `client-per-request` needs a negative fixture proving a module-top-level singleton is never scanned at
//! all: `MethodScan` only evaluates `SourceFile::symbols` body spans, and a top-level statement has no
//! enclosing function span — see `module_top_level_singleton_prisma_client_is_not_flagged`.
//!
//! Every rule's `suppress_marker` is exercised once below, with the marker directly above the
//! reported/trigger line (`MARKER_LOOKBACK_LINES` = 1 — the only lookback distance that suppresses).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent).
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

/// Loads the real `be-db.json` pack, filtered so this test is unaffected by sibling packs under
/// concurrent development.
fn be_db_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "be-db")
        .expect("be-db pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "be-db-fixture".to_string(),
        packs: vec![be_db_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("be-db/{rule}"))
        .collect()
}

// Test modules (split by rule/theme; the fixtures above are shared via `use super::*;`).
mod client_lifecycle;
mod money_and_catch;
mod queries;
mod races;
mod transactions;
mod writes;
