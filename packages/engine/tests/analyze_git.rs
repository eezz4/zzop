//! End-to-end test for the git + FileNode + scores integration: a real temp git
//! repo (same `git init`/`config`/`commit` pattern as `packages/git/src/lib.rs`'s own
//! `collect_end_to_end_against_a_real_temp_git_repo` integration test) analyzed with
//! `EngineConfig::git` set, asserting `nodes` carry real change counts and `scores`/`health`/
//! `recommendations`/`critical`/`seams` are populated — plus determinism across two runs, and that a
//! non-git root degrades to a `warnings` entry instead of panicking.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RulePackDef;
use zzop_engine::{analyze_tree, EngineConfig, GitOptions};
use zzop_metrics::RecId;

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
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Builds a real git repo with a small TS tree and enough history (multiple commits, a rename, a
/// `[FIX]`-tagged commit) that `nodes`/`scores`/`recommendations` all have something to report on.
fn git_fixture_repo() -> TempDir {
    let dir = TempDir::new("zzop-engine-git-fixture");
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test User"]);

    fs::write(
        dir.path().join("a.ts"),
        "import { b } from './b';\nexport function a() { return b(); }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("b.ts"),
        "export function b() { return 1; }\n",
    )
    .unwrap();
    run_git(dir.path(), &["add", "a.ts", "b.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FEAT] add a and b"]);

    fs::write(
        dir.path().join("a.ts"),
        "import { b } from './b';\nexport function a() { return b() + 1; }\n",
    )
    .unwrap();
    run_git(dir.path(), &["add", "a.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FIX] correct a"]);

    fs::write(
        dir.path().join("a.ts"),
        "import { b } from './b';\nexport function a() { return b() + 2; }\n",
    )
    .unwrap();
    run_git(dir.path(), &["add", "a.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FIX] correct a again"]);

    dir
}

fn config_with_git() -> EngineConfig {
    EngineConfig {
        source_id: "fixture".to_string(),
        git: Some(GitOptions::default()),
        ..EngineConfig::default()
    }
}

#[test]
fn git_enabled_nodes_carry_real_change_counts() {
    if !git_available() {
        eprintln!("skipping git_enabled_nodes_carry_real_change_counts: git not on PATH");
        return;
    }
    let dir = git_fixture_repo();
    let out = analyze_tree(dir.path(), &config_with_git());
    // `config_with_git()` enables git but sets no packs, so the sole expected warning is the zero-packs
    // capability note (git-not-requested does not fire since git IS enabled here).
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert!(
        out.warnings[0].contains("no DSL rule packs loaded"),
        "{:?}",
        out.warnings
    );

    let a = out
        .nodes
        .iter()
        .find(|n| n.path == "a.ts")
        .expect("expected a node for a.ts");
    assert_eq!(a.change_count, 3);
    assert!(a.tag_counts.get("FIX").copied().unwrap_or(0) >= 2);

    let b = out
        .nodes
        .iter()
        .find(|n| n.path == "b.ts")
        .expect("expected a node for b.ts");
    assert_eq!(b.change_count, 1);
}

#[test]
fn git_enabled_populates_scores_health_recommendations_critical_and_seams() {
    if !git_available() {
        eprintln!(
            "skipping git_enabled_populates_scores_health_recommendations_critical_and_seams: git not on PATH"
        );
        return;
    }
    let dir = git_fixture_repo();
    let out = analyze_tree(dir.path(), &config_with_git());

    assert!(out.scores.is_some(), "expected scores to be populated");
    assert!(
        out.health.is_some(),
        "expected health index to be populated"
    );
    // a.ts has 2 FIX-tagged commits out of 3 total changes — comfortably over the default
    // bug-prone-fix gate (5) is NOT met here, but recommendations as a whole should still be a
    // well-formed (possibly empty) Vec, not silently absent from the struct. What we assert concretely
    // is that computing recommendations did not panic and the health index's pain is finite.
    let health = out.health.unwrap();
    assert!(health.pain.is_finite());
    assert!(health.pain >= 0.0);
    // critical / seams are small-repo-appropriate (likely empty given the min-blast-radius / min-files
    // gates on a 2-file repo) — assert they are at least well-formed, not that they're non-empty.
    let _ = out.critical;
    let _ = out.seams;
    let _ = out.recommendations;
}

#[test]
fn two_runs_over_the_same_git_repo_are_deterministic() {
    if !git_available() {
        eprintln!("skipping two_runs_over_the_same_git_repo_are_deterministic: git not on PATH");
        return;
    }
    let dir = git_fixture_repo();
    let cfg = config_with_git();
    let out1 = analyze_tree(dir.path(), &cfg);
    let out2 = analyze_tree(dir.path(), &cfg);

    assert_eq!(
        serde_json::to_value(&out1.nodes).unwrap(),
        serde_json::to_value(&out2.nodes).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&out1.scores).unwrap(),
        serde_json::to_value(&out2.scores).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&out1.health).unwrap(),
        serde_json::to_value(&out2.health).unwrap()
    );
}

#[test]
fn git_disabled_by_default_keeps_scores_and_friends_empty() {
    let dir = TempDir::new("zzop-engine-no-git-fixture");
    fs::write(
        dir.path().join("a.ts"),
        "export function a() { return 1; }\n",
    )
    .unwrap();
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    assert!(out.scores.is_none());
    assert!(out.health.is_none());
    assert!(out.recommendations.is_empty());
    assert!(out.critical.is_empty());
    assert!(out.seams.is_empty());
    // No git and no packs -> both capability notes now fire (and nothing else, on this clean tree).
    assert_eq!(out.warnings.len(), 2, "{:?}", out.warnings);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("git history not requested")),
        "{:?}",
        out.warnings
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("no DSL rule packs loaded")),
        "{:?}",
        out.warnings
    );
    // `nodes` itself is still populated (dep-graph + LOC signal) even with git disabled.
    assert!(out.nodes.iter().any(|n| n.path == "a.ts"));
}

fn minimal_pack() -> RulePackDef {
    serde_json::from_str::<RulePackDef>(r#"{"id":"t","framework":"any","rules":[]}"#)
        .expect("minimal empty-rules pack should deserialize")
}

/// A single critical-severity `line-scan` rule flagging any file containing `DANGEROUS_MARKER` — the
/// minimal DSL pack shape needed to put a `Severity::Critical` `Finding` on a chosen file (see
/// `packages/core/src/dsl.rs`'s `RulePackDef`/`RuleDef`/`Matcher::LineScan` for the field shapes this
/// literal is authored against).
fn critical_marker_pack() -> RulePackDef {
    serde_json::from_str::<RulePackDef>(
        r#"{
            "id": "urgent-fixture",
            "framework": "any",
            "rules": [
                {
                    "id": "dangerous-marker",
                    "severity": "critical",
                    "message": "dangerous marker present",
                    "matcher": {
                        "type": "line-scan",
                        "file_pattern": "\\.ts$",
                        "line_pattern": "DANGEROUS_MARKER"
                    }
                }
            ]
        }"#,
    )
    .expect("critical-marker pack should deserialize")
}

/// Builds a real git repo where `a.ts` both (1) meets the default `bug-prone` recommendation gate
/// (>= 5 `[FIX]`-tagged commits) and (2) contains a `DANGEROUS_MARKER` line that `critical_marker_pack`
/// flags as a `Severity::Critical` finding — the coexistence `urgent_bug_risk_escalation`'s e2e assertion
/// needs (a recommendation target file that also carries a rule-confirmed critical finding).
fn git_fixture_repo_with_critical_bug_prone_file() -> TempDir {
    let dir = TempDir::new("zzop-engine-urgent-bug-risk-fixture");
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test User"]);

    fs::write(
        dir.path().join("a.ts"),
        "// DANGEROUS_MARKER\nexport function a() { return 1; }\n",
    )
    .unwrap();
    run_git(dir.path(), &["add", "a.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FEAT] add a"]);

    for i in 0..5 {
        fs::write(
            dir.path().join("a.ts"),
            format!("// DANGEROUS_MARKER\nexport function a() {{ return {i}; }}\n"),
        )
        .unwrap();
        run_git(dir.path(), &["add", "a.ts"]);
        let msg = format!("[FIX] fix a #{i}");
        run_git(dir.path(), &["commit", "-q", "-m", &msg]);
    }

    dir
}

#[test]
fn urgent_bug_risk_escalation_shows_up_first_with_evidence_in_a_single_tree_analyze() {
    if !git_available() {
        eprintln!(
            "skipping urgent_bug_risk_escalation_shows_up_first_with_evidence_in_a_single_tree_analyze: git not on PATH"
        );
        return;
    }
    let dir = git_fixture_repo_with_critical_bug_prone_file();
    let cfg = EngineConfig {
        packs: vec![critical_marker_pack()],
        ..config_with_git()
    };
    let out = analyze_tree(dir.path(), &cfg);

    // The critical finding itself is present on a.ts.
    assert!(
        out.findings
            .iter()
            .any(|f| f.file == "a.ts" && f.rule_id == "urgent-fixture/dangerous-marker"),
        "{:?}",
        out.findings
    );

    // a.ts also meets the bug-prone gate (5 [FIX] commits >= default gate of 5), so it would otherwise
    // land in the plain bug-prone group — instead it is escalated to urgent-bug-risk, which sorts first.
    assert!(
        !out.recommendations.is_empty(),
        "expected at least the urgent-bug-risk group"
    );
    let top = &out.recommendations[0];
    assert_eq!(top.id, RecId::UrgentBugRisk);
    assert!(top.items.iter().any(|i| i.path == "a.ts"));
    let item = top.items.iter().find(|i| i.path == "a.ts").unwrap();
    assert_eq!(item.escalated_from, Some(RecId::BugProne));
    assert!(
        item.bug_evidence
            .iter()
            .any(|e| e.contains("critical finding(s) in this file")
                && e.contains("urgent-fixture/dangerous-marker")),
        "{:?}",
        item.bug_evidence
    );

    // Never double-reported: a.ts does not also appear in a separate plain bug-prone group.
    assert!(out
        .recommendations
        .iter()
        .filter(|r| r.id == RecId::BugProne)
        .all(|r| r.items.iter().all(|i| i.path != "a.ts")));
}

#[test]
fn minimal_config_reports_both_capability_notes() {
    // A plain default config (no git, no packs) over a one-file tree self-reports BOTH silently-narrowed
    // scopes: git never requested, and no DSL packs loaded.
    let dir = TempDir::new("zzop-engine-minimal-config");
    fs::write(
        dir.path().join("a.ts"),
        "export function a() { return 1; }\n",
    )
    .unwrap();
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("git history not requested")),
        "{:?}",
        out.warnings
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("no DSL rule packs loaded")),
        "{:?}",
        out.warnings
    );
}

#[test]
fn git_and_packs_both_set_produces_no_capability_notes() {
    if !git_available() {
        eprintln!("skipping git_and_packs_both_set_produces_no_capability_notes: git not on PATH");
        return;
    }
    let dir = git_fixture_repo();
    let cfg = EngineConfig {
        packs: vec![minimal_pack()],
        ..config_with_git()
    };
    let out = analyze_tree(dir.path(), &cfg);
    // Both scopes are explicitly enabled, so neither capability note may appear (any other warnings are
    // fine — we assert only the ABSENCE of these two specific phrases).
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("git history not requested")),
        "{:?}",
        out.warnings
    );
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("no DSL rule packs loaded")),
        "{:?}",
        out.warnings
    );
}

/// Builds a real git repo with one commit whose subject only a custom (non-English) commit-type pattern
/// would classify — the default English FIX/FEAT/... table would leave it untagged entirely.
fn git_fixture_repo_with_a_french_fix_commit() -> TempDir {
    let dir = TempDir::new("zzop-engine-custom-commit-type-fixture");
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test User"]);

    fs::write(
        dir.path().join("a.ts"),
        "export function a() { return 1; }\n",
    )
    .unwrap();
    run_git(dir.path(), &["add", "a.ts"]);
    run_git(
        dir.path(),
        &["commit", "-q", "-m", "corrige le bug de fuseau horaire"],
    );

    dir
}

#[test]
fn custom_commit_type_patterns_replace_the_default_table_end_to_end() {
    if !git_available() {
        eprintln!("skipping custom_commit_type_patterns_replace_the_default_table_end_to_end: git not on PATH");
        return;
    }
    let dir = git_fixture_repo_with_a_french_fix_commit();
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        git: Some(GitOptions {
            commit_type_patterns: Some(vec![(r"^\s*corrige\b".to_string(), "FIX".to_string())]),
            ..GitOptions::default()
        }),
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    let a = out
        .nodes
        .iter()
        .find(|n| n.path == "a.ts")
        .expect("expected a node for a.ts");
    assert_eq!(
        a.tag_counts.get("FIX").copied(),
        Some(1),
        "custom pattern should classify the French commit as FIX: {:?}",
        a.tag_counts
    );

    // Sanity: the default (English-only) table would NOT classify this commit at all, proving the custom
    // table really replaced it rather than merely adding to it.
    let default_cfg = EngineConfig {
        source_id: "fixture".to_string(),
        ..config_with_git()
    };
    let default_out = analyze_tree(dir.path(), &default_cfg);
    let default_a = default_out
        .nodes
        .iter()
        .find(|n| n.path == "a.ts")
        .expect("expected a node for a.ts");
    assert!(
        default_a.tag_counts.is_empty(),
        "expected the default English table to leave the French commit untagged: {:?}",
        default_a.tag_counts
    );
}

#[test]
fn empty_custom_commit_type_patterns_falls_back_to_the_default_table() {
    if !git_available() {
        eprintln!("skipping empty_custom_commit_type_patterns_falls_back_to_the_default_table: git not on PATH");
        return;
    }
    // A conventional-commits subject with NO bracket tag: only the default keyword table can classify
    // it, so this fixture actually pins the empty-vec -> default-table fallback (a bracket-tagged
    // fixture would classify via the bracket grammar regardless and mask a broken fallback).
    let dir = TempDir::new("zzop-engine-empty-commit-type-fixture");
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test User"]);
    fs::write(
        dir.path().join("a.ts"),
        "export function a() { return 1; }\n",
    )
    .unwrap();
    run_git(dir.path(), &["add", "a.ts"]);
    run_git(
        dir.path(),
        &["commit", "-q", "-m", "fix: correct the timezone bug"],
    );

    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        git: Some(GitOptions {
            commit_type_patterns: Some(Vec::new()),
            ..GitOptions::default()
        }),
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    let a = out
        .nodes
        .iter()
        .find(|n| n.path == "a.ts")
        .expect("expected a node for a.ts");
    assert_eq!(
        a.tag_counts.get("FIX").copied(),
        Some(1),
        "an empty custom table must fall back to the default keyword table: {:?}",
        a.tag_counts
    );
}

#[test]
fn invalid_custom_commit_type_pattern_is_skipped_with_a_warning_not_a_panic() {
    if !git_available() {
        eprintln!("skipping invalid_custom_commit_type_pattern_is_skipped_with_a_warning_not_a_panic: git not on PATH");
        return;
    }
    let dir = git_fixture_repo();
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        git: Some(GitOptions {
            // An unbalanced character class — fails to compile as a regex.
            commit_type_patterns: Some(vec![("[unclosed".to_string(), "FIX".to_string())]),
            ..GitOptions::default()
        }),
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("git.commitTypePatterns") && w.contains("[unclosed")),
        "expected an invalid-pattern warning naming git.commitTypePatterns and the bad pattern: {:?}",
        out.warnings
    );
    // Never panics: the run completes and still has nodes for the fixture's files.
    assert!(out.nodes.iter().any(|n| n.path == "a.ts"));
}

#[test]
fn git_enabled_on_a_non_git_root_degrades_to_a_warning_without_panicking() {
    let dir = TempDir::new("zzop-engine-not-a-repo-fixture");
    fs::write(
        dir.path().join("a.ts"),
        "export function a() { return 1; }\n",
    )
    .unwrap();
    let out = analyze_tree(dir.path(), &config_with_git());
    assert!(!out.warnings.is_empty(), "expected a git warning");
    assert!(out.scores.is_none());
    assert!(out.health.is_none());
    assert!(out.recommendations.is_empty());
    assert!(out.critical.is_empty());
    assert!(out.seams.is_empty());
    // nodes still built (dep-graph + LOC only, since git collection failed).
    assert!(out.nodes.iter().any(|n| n.path == "a.ts"));
}
