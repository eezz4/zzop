//! Exercises `rules/dsl/perf/perf.json`'s `api-in-loop` method-scan rule end-to-end through
//! `zzop_engine::analyze_tree` against real swc-parsed TypeScript fixtures. A `// zzop-api-in-loop-ok` marker on
//! the finding's own line, or the line directly above, suppresses it via `RuleDef::suppress_marker`. Most
//! fixtures use a top-level function rather than a class method to avoid double-counting from overlapping
//! class/method spans; see `overlapping_class_and_method_spans_do_not_double_count` below for that case.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, Finding, RulePackDef};
use zzop_engine::{analyze_tree, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent, no `tempfile` dependency).
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

/// Loads the real `rules/dsl/perf/perf.json` from the repo, filtered to just the `perf` pack.
/// The pack this file's tests scan with. The disk load below parses EVERY pack JSON in the directory
/// and throws all but this one away, so doing it per test cost this binary that work once per test; the
/// `OnceLock` makes it once per binary. How many packs that is is not written here: it moved inside one
/// release (v0.30.0 exported a whole pack) and the sentence needs no size to make its point — the same
/// spelling `examples/packs/tests/sql_preferences.rs` already uses. The clone is cheap and — importantly
/// — SHARES the pack's compiled-regex memo (`zzop_core::dsl::RegexCache`), so the second test onward
/// also skips recompiling every pattern.
fn perf_pack() -> RulePackDef {
    static PACK: std::sync::OnceLock<RulePackDef> = std::sync::OnceLock::new();
    PACK.get_or_init(perf_pack_uncached).clone()
}

fn perf_pack_uncached() -> RulePackDef {
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
        .find(|p| p.id == "perf")
        .expect("perf.json pack present")
}

/// Runs the fused engine over a single-file fixture tree, returning just this rule's findings.
fn scan(rel: &str, content: &str) -> Vec<Finding> {
    let dir = TempDir::new("zzop-perf-apiloop");
    dir.write(rel, content);
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![perf_pack()],
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    out.findings
        .into_iter()
        .filter(|f| f.rule_id == "perf/api-in-loop")
        .collect()
}

fn snippet(f: &Finding) -> String {
    f.data.as_ref().unwrap()["snippet"]
        .as_str()
        .unwrap()
        .to_string()
}

mod api_in_loop;
