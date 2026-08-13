//! End-to-end tests for `rules/dsl/security/security.json`, exercised via `zzop_engine::analyze_tree`
//! so `Matcher::MethodScan` rules run against real parser-derived
//! `SourceSymbol` body spans (TypeScript via swc), not hand-built spans. Each rule below has at least
//! one positive fixture (asserting finding count AND line number) and one realistic negative
//! (near-miss) fixture; a handful of cases also exercise `suppress_marker`.

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

/// Loads the real `rules/dsl/security/security.json` from the repo, filtered to just the `security` pack
/// so this test is unaffected by sibling packs under concurrent development (same convention as
/// `http/http.rs`).
///
/// `CARGO_MANIFEST_DIR` is the `rules` crate root (`rules/Cargo.toml`), so `dsl/` is `rules/dsl` — this
/// pack's own `security.json` lives one level down, at `rules/dsl/security/security.json`.
/// The pack this file's tests scan with. The disk load below parses EVERY pack JSON in the directory
/// and throws all but this one away, so doing it per test cost this binary that work once per test; the
/// `OnceLock` makes it once per binary. How many packs that is is not written here: it moved inside one
/// release (v0.30.0 exported a whole pack) and the sentence needs no size to make its point — the same
/// spelling `examples/packs/tests/sql_preferences.rs` already uses. The clone is cheap and — importantly
/// — SHARES the pack's compiled-regex memo (`zzop_core::dsl::RegexCache`), so the second test onward
/// also skips recompiling every pattern.
fn security_pack() -> RulePackDef {
    static PACK: std::sync::OnceLock<RulePackDef> = std::sync::OnceLock::new();
    PACK.get_or_init(security_pack_uncached).clone()
}

fn security_pack_uncached() -> RulePackDef {
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
        .find(|p| p.id == "security")
        .expect("security pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "security-fixture".to_string(),
        packs: vec![security_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("security/{rule}"))
        .collect()
}

fn label_of(f: &zzop_core::Finding) -> Option<&str> {
    f.data
        .as_ref()
        .and_then(|d| d.get("label"))
        .and_then(|v| v.as_str())
}

mod conn_string_credentials;
mod cors_csp;
mod crypto;
mod frontend_exposure;
mod html_injection;
mod http_exposure;
mod java_moved_rules;
mod java_security;
mod jwt;
mod jwt_sign_secret;
mod mass_assignment;
mod private_key_committed;
mod request_targets;
mod rust_rules;
mod scan_scope;
mod secrets;
mod secrets_vetoes;
mod shell_exec;
mod sql_injection;
mod taint_and_eval;
mod template_output;
mod timing_compare;
mod vendor_token_committed;
