//! End-to-end coverage for `rules/dsl/egress/egress.json` plus the native `duplicate-route` analysis (`zzop_rules_http::duplicate_route_findings`, registered in `zzop_rules_http::register_native_analyses`).
//!
//! `duplicate-route` is native, not a DSL rule, so its findings carry the plain id `"duplicate-route"` (no `egress/` prefix) despite running alongside the pack here.

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

/// Loads the real `egress.json` pack, filtered so this test is unaffected by sibling packs under concurrent development.
/// The pack this file's tests scan with. The disk load below parses EVERY pack JSON in the directory
/// and throws all but this one away, so doing it per test cost this binary that work once per test; the
/// `OnceLock` makes it once per binary. How many packs that is is not written here: it moved inside one
/// release (v0.30.0 exported a whole pack) and the sentence needs no size to make its point — the same
/// spelling `examples/packs/tests/sql_preferences.rs` already uses. The clone is cheap and — importantly
/// — SHARES the pack's compiled-regex memo (`zzop_core::dsl::RegexCache`), so the second test onward
/// also skips recompiling every pattern.
fn egress_pack() -> RulePackDef {
    static PACK: std::sync::OnceLock<RulePackDef> = std::sync::OnceLock::new();
    PACK.get_or_init(egress_pack_uncached).clone()
}

fn egress_pack_uncached() -> RulePackDef {
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
        .find(|p| p.id == "egress")
        .expect("egress pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "egress-fixture".to_string(),
        packs: vec![egress_pack()],
        ..EngineConfig::default()
    }
}

fn config_with(rule_config: zzop_core::RuleConfig) -> EngineConfig {
    EngineConfig {
        source_id: "egress-fixture".to_string(),
        packs: vec![egress_pack()],
        rule_config,
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("egress/{rule}"))
        .collect()
}

// Test modules (split by rule/theme; the fixtures above are shared via `use super::*;`).
mod generated_banner_exemption;
mod http_shapes;
mod sockets_and_routes;
