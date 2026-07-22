//! End-to-end tests for `rules/dsl/be-reliability/be-reliability.json` — exercised via `zzop_engine::analyze_tree` so `Matcher::MethodScan` rules run against real parser-derived `SourceSymbol` body spans (not hand-built spans), same convention as `sql/sql.rs`/`http/http.rs`.
//!
//! Covers all rules in the pack: `async-route-no-catch`, `sync-fs-in-handler`, `await-in-map`, `promise-all-writes`, `json-parse-no-try`, `fetch-no-timeout`, `process-exit-in-lib`, `emitter-async-listener`, `promise-race-resource-leak`, `fs-check-then-use`, `stream-open-no-close-in-loop`, `listener-subscribe-in-loop` (method-scan; the last two via `trigger_in_loop` loop-span containment — see `perf/api-in-loop`'s convention); `env-nonnull-assert`, `debug-true-committed`, `body-limit-missing`, `console-in-be`, `interval-no-clear` (line-scan, uses the `require_file_absent` DSL extension), `env-outside-config`, `await-inside-promise-all-array` (line-scan).
//!
//! `fetch-no-timeout` scopes to backend files via a content-based `require_file` pre-gate (server-framework import / server-runtime API / Workers module shape / D1 prepared-statement call) rather than a path heuristic, so a standalone backend repo with no `be`/`api`/`server`-ish path segment is still in scope.
//!
//! Each rule has >=1 positive fixture (count + line asserted), >=1 realistic negative, and at least one `suppress_marker` case is covered.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

mod config_flags;
mod env_outside_config;
mod fetch_and_process;
mod routes_and_handlers;
mod server_hygiene;
mod suppression;
mod writes_and_parsing;

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

/// Loads the real `rules/dsl/be-reliability/be-reliability.json` from the repo, filtered to just the `be-reliability` pack so this test is unaffected by sibling packs under concurrent development (same convention as `http/http.rs`).
///
/// `CARGO_MANIFEST_DIR` is the `rules` crate root (`rules/Cargo.toml`), so `dsl/` is `rules/dsl` — this pack's own `be-reliability.json` lives one level down, at `rules/dsl/be-reliability/be-reliability.json`.
fn be_reliability_pack() -> RulePackDef {
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
        .find(|p| p.id == "be-reliability")
        .expect("be-reliability pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "be-reliability-fixture".to_string(),
        packs: vec![be_reliability_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("be-reliability/{rule}"))
        .collect()
}
