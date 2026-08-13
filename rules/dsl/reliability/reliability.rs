//! End-to-end tests for `rules/dsl/reliability/reliability.json` — exercised via `zzop_engine::analyze_tree` so `Matcher::MethodScan` rules run against real parser-derived `SourceSymbol` body spans (not hand-built spans), same convention as `sql/sql.rs`/`http/http.rs`.
//!
//! Covers all rules in the pack: `async-route-no-catch`, `sync-fs-in-handler`, `map-async-no-promise-all`, `promise-all-and-writes`, `json-parse-no-try`, `fetch-no-timeout`, `emitter-async-listener`, `fs-check-then-use`, `stream-open-no-close-in-loop`, `listener-subscribe-in-loop` (method-scan; the last two via `trigger_in_loop` loop-span containment — see `perf/api-in-loop`'s convention); `debug-true-committed`, `body-limit-missing`, `interval-no-clear` (line-scan, uses the `require_file_absent` DSL extension), `await-inside-promise-all-array` (line-scan); `reqwest-no-timeout` (method-scan, `.rs`).
//!
//! SIX rules left this pack on 2026-08-12 — `env-nonnull-assert`, `process-exit-in-lib`, `console-in-be`, `console-in-loop`, `env-outside-config`, `promise-race-no-cancel` — exported to `examples/packs/code-hygiene.json` as the last increment of the `axis: opinion` export. Their tests went with them (`examples/packs/tests/`), and so did the whole Mode-B overlay helper set (`scan_with`, `env_config_overlay`, `deny_env_config_for_file`), which existed only for the declaration-gated `env-outside-config` and had no consumer left here. That also means this pack no longer reads the projected `call_sites` channel at all: all three of its call-scan rules were among the six, so the Python/Go/Java/C#/Rust fixtures that used to live here are now in `examples/packs/tests/w2_languages.rs`.
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
mod fetch_and_process;
mod routes_and_handlers;
mod rust_reqwest;
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

/// Loads the real `rules/dsl/reliability/reliability.json` from the repo, filtered to just the `reliability` pack so this test is unaffected by sibling packs under concurrent development (same convention as `http/http.rs`).
///
/// `CARGO_MANIFEST_DIR` is the `rules` crate root (`rules/Cargo.toml`), so `dsl/` is `rules/dsl` — this pack's own `reliability.json` lives one level down, at `rules/dsl/reliability/reliability.json`.
/// The pack this file's tests scan with. The disk load below parses EVERY pack JSON in the directory
/// and throws all but this one away, so doing it per test cost this binary that work once per test; the
/// `OnceLock` makes it once per binary. How many packs that is is not written here: it moved inside one
/// release (v0.30.0 exported a whole pack) and the sentence needs no size to make its point — the same
/// spelling `examples/packs/tests/sql_preferences.rs` already uses. The clone is cheap and — importantly
/// — SHARES the pack's compiled-regex memo (`zzop_core::dsl::RegexCache`), so the second test onward
/// also skips recompiling every pattern.
fn reliability_pack() -> RulePackDef {
    static PACK: std::sync::OnceLock<RulePackDef> = std::sync::OnceLock::new();
    PACK.get_or_init(reliability_pack_uncached).clone()
}

fn reliability_pack_uncached() -> RulePackDef {
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
        .find(|p| p.id == "reliability")
        .expect("reliability pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "reliability-fixture".to_string(),
        packs: vec![reliability_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("reliability/{rule}"))
        .collect()
}
