//! End-to-end coverage that `EngineConfig::rule_config`'s `severity_overrides` and `suppressions` fields
//! take effect through a real `analyze_tree` run (not just `zzop_core::registry`'s unit tests). These are
//! the two knobs the napi `analyze()` request surface (`severityOverrides`/`suppressions`) populates and
//! threads into `RuleConfig`; this file proves the engine's finalize path (`merge_findings`'s
//! `apply_severity_override` / `is_suppressed`) honors them against real findings.
//!
//! Uses the same hand-rolled `TempDir` pattern as the sibling `analyze_diagnostics.rs`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{RuleConfig, Severity, Suppression};
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

/// Two files importing each other — a dependency cycle the native `circular` analysis flags with default
/// severity `warning`.
fn cycle_fixture() -> TempDir {
    let dir = TempDir::new("zzop-engine-rule-config");
    dir.write(
        "a.ts",
        "import { b } from './b';\nexport function a() { return b(); }\n",
    );
    dir.write(
        "b.ts",
        "import { a } from './a';\nexport function b() { return a(); }\n",
    );
    dir
}

#[test]
fn severity_overrides_remap_a_finding_through_analyze_tree() {
    let dir = cycle_fixture();

    // Baseline: `circular` fires at its default severity (`warning`).
    let baseline = analyze_tree(dir.path(), &EngineConfig::default());
    let circular = baseline
        .findings
        .iter()
        .find(|f| f.rule_id == "circular")
        .expect("baseline run should produce a circular finding");
    assert_eq!(circular.severity, Severity::Warning);

    // With an override, the same finding must come back as `critical`.
    let mut overrides = BTreeMap::new();
    overrides.insert("circular".to_string(), Severity::Critical);
    let cfg = EngineConfig {
        rule_config: RuleConfig {
            severity_overrides: overrides,
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    let circular = out
        .findings
        .iter()
        .find(|f| f.rule_id == "circular")
        .expect("override run should still produce a circular finding");
    assert_eq!(
        circular.severity,
        Severity::Critical,
        "severity_overrides must remap circular warning -> critical, got: {:?}",
        out.findings
    );
}

#[test]
fn suppressions_drop_a_finding_through_analyze_tree() {
    let dir = cycle_fixture();

    // Sanity: without suppression the fixture emits a circular finding.
    let baseline = analyze_tree(dir.path(), &EngineConfig::default());
    assert!(
        baseline.findings.iter().any(|f| f.rule_id == "circular"),
        "baseline run should produce a circular finding, got: {:?}",
        baseline.findings
    );

    let cfg = EngineConfig {
        rule_config: RuleConfig {
            suppressions: vec![Suppression {
                rule: "circular".to_string(),
                path: None,
                glob: None,
            }],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        !out.findings.iter().any(|f| f.rule_id == "circular"),
        "suppressions must drop the circular finding, got: {:?}",
        out.findings
    );
}
