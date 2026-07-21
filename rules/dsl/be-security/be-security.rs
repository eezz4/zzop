//! End-to-end tests for `rules/dsl/be-security/be-security.json` (41 backend-security rules), exercised via
//! `zzop_engine::analyze_tree` so `Matcher::MethodScan` rules run against real parser-derived
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

/// Loads the real `rules/dsl/be-security/be-security.json` from the repo, filtered to just the `be-security` pack
/// so this test is unaffected by sibling packs under concurrent development (same convention as
/// `http/http.rs`).
///
/// `CARGO_MANIFEST_DIR` is the `rules` crate root (`rules/Cargo.toml`), so `dsl/` is `rules/dsl` — this
/// pack's own `be-security.json` lives one level down, at `rules/dsl/be-security/be-security.json`.
fn be_security_pack() -> RulePackDef {
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
        .find(|p| p.id == "be-security")
        .expect("be-security pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "be-security-fixture".to_string(),
        packs: vec![be_security_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("be-security/{rule}"))
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
mod scan_scope;
mod secrets;
mod secrets_vetoes;
mod shell_exec;
mod sql_injection;
mod template_output;
mod timing_compare;
mod vendor_token_committed;
