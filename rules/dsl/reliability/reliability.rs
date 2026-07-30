//! End-to-end tests for `rules/dsl/reliability/reliability.json` — exercised via `zzop_engine::analyze_tree` so `Matcher::MethodScan` rules run against real parser-derived `SourceSymbol` body spans (not hand-built spans), same convention as `sql/sql.rs`/`http/http.rs`.
//!
//! Covers all rules in the pack: `async-route-no-catch`, `sync-fs-in-handler`, `map-async-no-promise-all`, `promise-all-and-writes`, `json-parse-no-try`, `fetch-no-timeout`, `process-exit-in-lib`, `emitter-async-listener`, `promise-race-no-cancel`, `fs-check-then-use`, `stream-open-no-close-in-loop`, `listener-subscribe-in-loop` (method-scan; the last two via `trigger_in_loop` loop-span containment — see `perf/api-in-loop`'s convention); `env-nonnull-assert`, `debug-true-committed`, `body-limit-missing`, `console-in-be`, `interval-no-clear` (line-scan, uses the `require_file_absent` DSL extension), `env-outside-config`, `await-inside-promise-all-array` (line-scan).
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

/// Loads the real `rules/dsl/reliability/reliability.json` from the repo, filtered to just the `reliability` pack so this test is unaffected by sibling packs under concurrent development (same convention as `http/http.rs`).
///
/// `CARGO_MANIFEST_DIR` is the `rules` crate root (`rules/Cargo.toml`), so `dsl/` is `rules/dsl` — this pack's own `reliability.json` lives one level down, at `rules/dsl/reliability/reliability.json`.
fn reliability_pack() -> RulePackDef {
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

/// `scan` with one Mode-B adapter overlay attached — the channel a `zzop.config.jsonc` author reaches
/// through the `overlays` key, and the only way a declaration-gated rule (`env-outside-config`) can be
/// exercised in its ENABLED state. Kept beside `scan` rather than inside the one test file that needs it:
/// the gate is a general line-scan capability, so the next rule to use one starts here.
fn scan_with(dir: &TempDir, overlay: zzop_core::NormalizedEnvelope) -> AnalyzeOutput {
    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay];
    analyze_tree(dir.path(), &cfg)
}

/// An attributes-only overlay declaring each entry of `prefixes` an `env-config-module`. A path that
/// names a directory is a covering `pathScope`; the same call is used for an exact file path, which
/// resolves as the more specific `pathScope` of that exact string — `deny_env_config_for_file` is what
/// adds a genuine exact-`file` target.
///
/// The envelope carries a single synthetic file entry, because `AttributeStore::from_parts` flattens
/// `files[].attributes` tree-wide and never cares which file emitted them — the same shape
/// `examples/adapters/auth-overlay-adapter` emits.
fn env_config_overlay(prefixes: &[&str]) -> zzop_core::NormalizedEnvelope {
    overlay_with_attributes(
        prefixes
            .iter()
            .map(|p| zzop_core::Attribute {
                target: zzop_core::EntityRef::PathScope {
                    prefix: (*p).to_string(),
                },
                key: "env-config-module".to_string(),
                value: serde_json::Value::Bool(true),
            })
            .collect(),
    )
}

/// Appends an exact-`file` `env-config-module: false` to `overlay` — the carve-out spelling: an exact
/// target beats every covering scope, so this un-declares one file inside a declared directory.
fn deny_env_config_for_file(overlay: &mut zzop_core::NormalizedEnvelope, path: &str) {
    overlay.files[0].attributes.push(zzop_core::Attribute {
        target: zzop_core::EntityRef::File {
            path: path.to_string(),
        },
        key: "env-config-module".to_string(),
        value: serde_json::Value::Bool(false),
    });
}

fn overlay_with_attributes(attributes: Vec<zzop_core::Attribute>) -> zzop_core::NormalizedEnvelope {
    zzop_core::NormalizedEnvelope {
        format: zzop_core::NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "reliability-fixture-declarations/1".to_string(),
        source: String::new(),
        files: vec![zzop_core::FileProjection {
            path: "zzop-attributes.json".to_string(),
            loc: 1,
            attributes,
            ..Default::default()
        }],
    }
}

/// Every `AnalyzeOutput::warnings` entry mentioning `needle` — the disclosure channel a silenced rule
/// reports through.
fn warnings_matching<'a>(out: &'a AnalyzeOutput, needle: &str) -> Vec<&'a String> {
    out.warnings.iter().filter(|w| w.contains(needle)).collect()
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("reliability/{rule}"))
        .collect()
}
