//! End-to-end tests for `rules/dsl/go/go.json`, exercised via `zzop_engine::analyze_tree` so
//! `Matcher::MethodScan`'s `trigger_in_loop` gate runs against real `zzop_parser_go`-derived loop spans
//! (not hand-built spans) — the same "real parser, not a stubbed fixture" discipline
//! `rules/dsl/be-db/be-db.rs`'s own module doc describes for TypeScript.
//!
//! `go-goroutine-in-loop`'s `suppress_marker` is exercised once below, with the marker directly above
//! the reported/trigger line (`MARKER_LOOKBACK_LINES` = 1 — the only lookback distance that suppresses),
//! mirroring `rules/dsl/be-db/be-db.rs`'s own convention.

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

/// Loads the real `go.json` pack, filtered so this test is unaffected by sibling packs under
/// concurrent development.
fn go_pack() -> RulePackDef {
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
        .find(|p| p.id == "go")
        .expect("go pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "go-fixture".to_string(),
        packs: vec![go_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("go/{rule}"))
        .collect()
}

// Test modules (split by rule/theme; the fixtures above are shared via `use super::*;`).
mod goroutine_in_loop;
