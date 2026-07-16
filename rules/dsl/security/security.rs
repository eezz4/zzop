//! Exercises `rules/dsl/security/security.json`'s `taint-flow` and `eval-dynamic-code` rules end-to-end
//! via `zzop_engine::analyze_tree` against real swc-parsed TypeScript/TSX fixtures. `taint-flow` is an
//! explicitly coarse approximation (source+sink co-occurrence within a method-scan span, no real
//! per-variable dataflow) — see the rule's `message` for the full list of precision limits.
//!
//! `eval-dynamic-code` is source-free and js-inclusive (unlike `taint-flow`, which needs a request-derived
//! source in the same function and only looks at `.ts`/`.tsx`): `eval(...)` with a non-literal argument
//! (`eval-nonliteral`) or any `new Function(...)` call, literal args included (`new-function`).

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

/// Loads the real `rules/dsl/security/security.json` from the repo, filtered to just the `security` pack.
fn security_pack() -> RulePackDef {
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

mod taint_and_eval;
