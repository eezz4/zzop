//! Exercises `rules/dsl/typescript/typescript.json`'s type-safety and unhandled-promise rules end-to-end
//! through `zzop_engine::analyze_tree` against real swc-parsed TypeScript fixtures. See the rule `message`
//! fields for full rationale: `as-cast` excludes import-alias `as` via `LineScan::exclude_pattern`;
//! `async-handler-no-try` vetoes via `MethodScan::absent` when a `try {` appears anywhere in the enclosing
//! symbol span (coarser than "wraps the specific async call"); `no-explicit-any` and `as-cast` both fire
//! one finding per matching *line*, not per occurrence.

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

/// Loads the real `rules/dsl/typescript/typescript.json` from the repo, filtered to just the `typescript` pack.
fn typescript_pack() -> RulePackDef {
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
        .find(|p| p.id == "typescript")
        .expect("typescript.json pack present")
}

fn analyze(files: &[(&str, &str)]) -> Vec<Finding> {
    let dir = TempDir::new("zzop-typescript-pack");
    for (rel, content) in files {
        dir.write(rel, content);
    }
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![typescript_pack()],
        ..EngineConfig::default()
    };
    analyze_tree(dir.path(), &cfg).findings
}

fn any_findings(files: &[(&str, &str)]) -> Vec<Finding> {
    analyze(files)
        .into_iter()
        .filter(|f| f.rule_id == "typescript/no-explicit-any")
        .collect()
}

fn lines_of(findings: &[Finding]) -> Vec<u32> {
    let mut v: Vec<u32> = findings.iter().map(|f| f.line).collect();
    v.sort_unstable();
    v
}

// --- float-equality / always-false-comparison / numeric-string-comparison / tofixed-arithmetic /
// date-pitfalls / foreach-async-callback / promise-async-executor / parseint-no-radix ---
//
// These 8 rules are plain-JS correctness bugs, not TS-type-system bugs, so unlike the 4 rules above they
// use the broadened `file_pattern` `(?i)\.(ts|tsx|js|jsx|mjs|cjs)$` — every fixture file below uses a
// `.js`/`.ts` mix on purpose to exercise that breadth.

/// Loads both the `typescript` and `be-db` packs together, for the `float-equality` /
/// `float-money-compare` dual-pack boundary fixture below (be-db.json ships in the same `dsl/` tree).
fn typescript_and_be_db_packs() -> Vec<RulePackDef> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    let mut packs: Vec<RulePackDef> = result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .filter(|p| p.id == "typescript" || p.id == "be-db")
        .collect();
    assert_eq!(packs.len(), 2, "{packs:?}");
    packs.sort_by(|a, b| a.id.cmp(&b.id));
    packs
}

fn analyze_with_packs(files: &[(&str, &str)], packs: Vec<RulePackDef>) -> Vec<Finding> {
    let dir = TempDir::new("zzop-typescript-be-db-pack");
    for (rel, content) in files {
        dir.write(rel, content);
    }
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs,
        ..EngineConfig::default()
    };
    analyze_tree(dir.path(), &cfg).findings
}

fn rule_findings(files: &[(&str, &str)], rule: &str) -> Vec<Finding> {
    analyze(files)
        .into_iter()
        .filter(|f| f.rule_id == format!("typescript/{rule}"))
        .collect()
}

// Test modules (split by rule/theme; the fixtures above are shared via `use super::*;`).
mod casts_and_handlers;
mod js_pitfalls;
mod no_explicit_any;
mod numeric_correctness;
