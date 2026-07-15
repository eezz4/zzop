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

#[test]
fn unknown_severity_override_id_surfaces_a_self_report_warning() {
    // A `severity_overrides` key that matches no known native-analysis id / "<pack>/<rule>" id silently
    // remaps nothing (`registry::apply_severity_override`'s exact-match-on-`finding.rule_id` contract) —
    // proves the honest-output side: the diagnostics self-report must still tell the user their typo'd key
    // never remapped a finding (see `zzop_engine::analyze::diagnostics::unknown_severity_override_ids`).
    let dir = cycle_fixture();

    let mut overrides = BTreeMap::new();
    overrides.insert("n-plus-one".to_string(), Severity::Critical);
    let cfg = EngineConfig {
        rule_config: RuleConfig {
            severity_overrides: overrides,
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);

    let matches: Vec<&String> = out
        .warnings
        .iter()
        .filter(|w| w.contains("matching no known rule id") && w.contains("severityOverrides"))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one unknown-severity-override-id self-report, got: {:?}",
        out.warnings
    );
    assert!(matches[0].contains("n-plus-one"));
}

#[test]
fn a_real_severity_override_id_does_not_trigger_the_unknown_id_warning() {
    // Sanity check for the other direction: a real, known id (a native analysis id here) must never be
    // reported as unknown — the check is a set-membership diff, not a "any severity_overrides entry
    // present" trigger.
    let dir = cycle_fixture();

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

    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("severityOverrides") && w.contains("matching no known rule id")),
        "a real, known severity_overrides id must not be reported as unknown, got: {:?}",
        out.warnings
    );
}

#[test]
fn unknown_suppression_rule_id_surfaces_a_self_report_warning() {
    // A `suppressions[].rule` that matches no known native-analysis id / "<pack>/<rule>" id silently
    // suppresses nothing (`registry::is_suppressed`'s exact-match-on-`entry.rule` contract) — proves the
    // honest-output side: the diagnostics self-report must still tell the user their typo'd rule id never
    // suppressed a finding (see `zzop_engine::analyze::diagnostics::unknown_suppression_rule_ids`).
    let dir = cycle_fixture();

    let cfg = EngineConfig {
        rule_config: RuleConfig {
            suppressions: vec![Suppression {
                rule: "n-plus-one".to_string(),
                path: None,
                glob: None,
            }],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);

    let matches: Vec<&String> = out
        .warnings
        .iter()
        .filter(|w| w.contains("suppressions have") && w.contains("matches no known rule id"))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one unknown-suppression-rule-id self-report, got: {:?}",
        out.warnings
    );
    assert!(matches[0].contains("n-plus-one"));
    assert!(matches[0].contains("did NOT suppress anything"));
}

#[test]
fn a_real_suppression_rule_id_does_not_trigger_the_unknown_id_warning() {
    // Sanity check for the other direction: a real, known id (a native analysis id here) must never be
    // reported as unknown — the check is a set-membership diff, not a "any suppressions entry present"
    // trigger.
    let dir = cycle_fixture();

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
        !out.warnings
            .iter()
            .any(|w| w.contains("suppressions have") && w.contains("matches no known rule id")),
        "a real, known suppressions rule id must not be reported as unknown, got: {:?}",
        out.warnings
    );
}

#[test]
fn unknown_suppression_rule_id_and_dead_filter_are_orthogonal_and_both_fire() {
    // A single `Suppression` entry can carry two independent mistakes at once: a typo'd `rule` (matches no
    // known rule id) AND a path/glob filter that matches no scanned file. These are checked by two separate
    // mechanisms (`unknown_suppression_rule_ids` vs. `unmatched_suppression_warnings`) and must both surface
    // — neither check should suppress the other.
    let dir = cycle_fixture();

    let cfg = EngineConfig {
        rule_config: RuleConfig {
            suppressions: vec![Suppression {
                rule: "n-plus-one".to_string(),
                path: Some("nonexistent/dir/".to_string()),
                glob: None,
            }],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        out.warnings.iter().any(|w| w.contains("suppressions have")
            && w.contains("matches no known rule id")
            && w.contains("n-plus-one")),
        "expected the unknown-rule-id self-report, got: {:?}",
        out.warnings
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("exclude for rule 'n-plus-one'") && w.contains("matched no files")),
        "expected the dead-filter self-report, got: {:?}",
        out.warnings
    );
}
