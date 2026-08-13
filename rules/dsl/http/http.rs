//! Exercises `rules/dsl/http/http.json`'s HTTP-route rules end-to-end via `zzop_engine::analyze_tree` against
//! real swc-parsed TypeScript fixtures. See `http.json` for each rule's exact matcher shape and message.
//!
//! The pack's third rule, `get-route-no-cache-marker`, was DELETED 2026-08-02 rather than tested around:
//! its `require_file` gate (`apiRoutes\.get\(`) was one repository's own router-variable vocabulary, so
//! for every other user the rule was eternally silent by construction — a house convention has no place
//! in a shipped pack (old id recorded in `VERSIONING.md`; a config naming it gets the standard
//! unknown-rule-id warning).
//!
//! Ordering-aware and graph-shaped route checks (auth-state-machine transitions, API churn, unsafe-read-endpoint,
//! non-idempotent-write, FE/BE spec drift) are out of scope for a per-file DSL matcher and stay on the native-analysis backlog.
//!
//! Both remaining rules are `io-scan` matchers over assembled `http` provides — framework-neutral, and
//! path-shape-free: their `file_pattern` is a language-extension gate, not a directory convention.

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

/// Loads the real `http.json` pack, filtered so this test is unaffected by sibling packs under concurrent development.
/// The pack this file's tests scan with. The disk load below parses EVERY pack JSON in the directory
/// and throws all but this one away, so doing it per test cost this binary that work once per test; the
/// `OnceLock` makes it once per binary. How many packs that is is not written here: it moved inside one
/// release (v0.30.0 exported a whole pack) and the sentence needs no size to make its point — the same
/// spelling `examples/packs/tests/sql_preferences.rs` already uses. The clone is cheap and — importantly
/// — SHARES the pack's compiled-regex memo (`zzop_core::dsl::RegexCache`), so the second test onward
/// also skips recompiling every pattern.
fn http_pack() -> RulePackDef {
    static PACK: std::sync::OnceLock<RulePackDef> = std::sync::OnceLock::new();
    PACK.get_or_init(http_pack_uncached).clone()
}

fn http_pack_uncached() -> RulePackDef {
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
        .find(|p| p.id == "http")
        .expect("http pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "http-fixture".to_string(),
        packs: vec![http_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("http/{rule}"))
        .collect()
}

mod dev_path_no_guard_hint;
mod protected_path_no_auth_evidence;
