//! A rule that cannot fire must SAY SO. `zzop_engine::analyze::diagnostics::uncompilable_rule_warnings`
//! (wired into both `analyze::assemble` and `envelope::ingest`) turns the two dead-rule shapes into
//! per-run `warnings` lines: a pattern that does not compile, and the structural shapes that leave a
//! matcher with nothing to match. Before this existed the evaluator skipped such a rule silently and the
//! run reported clean — the misleading-diagnosis failure this engine refuses to commit.
//!
//! These messages are a user-visible contract (`diagnostics::capability`'s own module doc says so), which
//! is why their shape is pinned here rather than left to inspection.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{parse_dsl_pack, RulePackDef};
use zzop_engine::{analyze_tree, EngineConfig};

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

fn pack(json: &str) -> RulePackDef {
    parse_dsl_pack(json).expect("fixture pack must parse — the defects here are post-parse ones")
}

fn config(packs: Vec<RulePackDef>) -> EngineConfig {
    EngineConfig {
        source_id: "uncompilable-rule-fixture".to_string(),
        packs,
        ..EngineConfig::default()
    }
}

fn tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-uncompilable-rule");
    dir.write("src/app.ts", "export const x = 1;\n");
    dir
}

/// The message names the PACK-QUALIFIED rule id, the offending field, and the fact that the rule is dead
/// — pack-qualified because this list spans every loaded pack, so a bare id could not be acted on.
#[test]
fn an_uncompilable_pattern_is_disclosed_with_the_pack_qualified_rule_id() {
    let dir = tree();
    let out = analyze_tree(
        dir.path(),
        &config(vec![pack(
            r#"{"id": "fixture", "rules": [
                {"id": "broken", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.ts$", "line_pattern": "(unclosed"}}
            ]}"#,
        )]),
    );

    let hit = out
        .warnings
        .iter()
        .find(|w| w.contains("fixture/broken"))
        .unwrap_or_else(|| panic!("expected a dead-rule warning, got: {:?}", out.warnings));
    assert!(hit.contains("`line_pattern`"), "{hit}");
    assert!(hit.contains("can never fire"), "{hit}");
    assert!(hit.contains("validate-rule-pack"), "{hit}");
}

/// The structural half: a line-scan with neither `line_pattern` nor `any` parses fine and is just as dead.
/// This is the shape that used to slip through `validate-rule-pack` with `{"valid": true}`.
#[test]
fn a_line_scan_with_nothing_to_match_is_disclosed_too() {
    let dir = tree();
    let out = analyze_tree(
        dir.path(),
        &config(vec![pack(
            r#"{"id": "fixture", "rules": [
                {"id": "empty", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.ts$"}}
            ]}"#,
        )]),
    );

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("fixture/empty") && w.contains("neither `line_pattern` nor `any`")),
        "expected the structural dead-rule warning, got: {:?}",
        out.warnings
    );
}

/// A healthy pack must stay silent — a disclosure channel that always fires teaches readers to ignore it.
#[test]
fn a_healthy_pack_produces_no_dead_rule_warning() {
    let dir = tree();
    let out = analyze_tree(
        dir.path(),
        &config(vec![pack(
            r#"{"id": "fixture", "rules": [
                {"id": "fine", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.ts$", "line_pattern": "export"}}
            ]}"#,
        )]),
    );

    assert!(
        !out.warnings.iter().any(|w| w.contains("can never fire")),
        "expected no dead-rule warning, got: {:?}",
        out.warnings
    );
}
