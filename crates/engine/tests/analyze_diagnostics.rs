//! End-to-end coverage for `zzop_metrics::diagnostics` wired into `analyze::assemble` — the self-report
//! half of the "silent degenerate data must self-report" principle. Uses the same hand-rolled `TempDir`
//! pattern as `crates/engine/tests/pack_sql.rs`.
//!
//! - A tree whose files parse but carry zero internal dep edges and zero exported symbols must surface
//!   both coverage-gap warnings on `AnalyzeOutput::warnings`.
//! - A healthy small tree (real import edge, real exported symbols) must stay warning-free — the
//!   diagnostics wiring must not manufacture noise on a legitimately clean run.
//! - A git-disabled run (the default `EngineConfig::git = None`) must never emit git-window diagnostics
//!   (0 commits / 0 changes / untagged commits) even when the underlying counts are honestly zero,
//!   since git was never attempted for this run — `analyze::run_diagnostics` passes
//!   `DiagnosticsInput::git = None` in that case, and `zzop_metrics::diagnostics::build_diagnostics`
//!   itself gates every git-window warning behind `git.is_some()`.
//! - A healthy tree whose top-level functions are simply never exported (no public API, not a parser
//!   failure) must still surface the 0-exported-symbols warning, but with wording that presents both
//!   possible causes rather than asserting detection failed.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RuleConfig;
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

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "diagnostics-fixture".to_string(),
        ..EngineConfig::default()
    }
}

/// Three TS files, none importing or exporting anything — every dep/symbol signal this repo could carry
/// is genuinely empty, not a fluke of a single tiny file (the module's "single-file package" exception
/// only suppresses the 0-edges warning for `files <= 1`).
fn disconnected_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-diag-disconnected");
    dir.write(
        "a.ts",
        "const x = 1;\nfunction helper() { return x + 1; }\n",
    );
    dir.write("b.ts", "const y = 2;\nfunction other() { return y * 2; }\n");
    dir.write("c.ts", "const z = 3;\nfunction third() { return z - 1; }\n");
    dir
}

#[test]
fn disconnected_tree_reports_zero_edges_and_zero_symbols() {
    let dir = disconnected_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("0 internal dependency edges") && w.contains("3 files")),
        "expected a 0-dep-edges self-report, got: {:?}",
        out.warnings
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("0 exported symbols")),
        "expected a 0-exported-symbols self-report, got: {:?}",
        out.warnings
    );
}

#[test]
fn healthy_small_fixture_produces_no_diagnostics_warnings() {
    let dir = TempDir::new("zzop-engine-diag-healthy");
    dir.write(
        "a.ts",
        "import { b } from './b';\nexport function a() { return b(); }\n",
    );
    dir.write("b.ts", "export function b() { return 1; }\n");

    let out = analyze_tree(dir.path(), &config());

    // This test's intent is that the DIAGNOSTICS wiring manufactures no noise on a clean tree — not that
    // `warnings` is globally empty. The two always-on capability notes (git never requested / no DSL packs
    // loaded, both fired by `config()`'s pack-less, git-less config) are filtered out; nothing else may
    // remain.
    let other: Vec<&String> = out
        .warnings
        .iter()
        .filter(|w| {
            !w.contains("git history not requested") && !w.contains("no DSL rule packs loaded")
        })
        .collect();
    assert!(
        other.is_empty(),
        "healthy fixture should be diagnostics-warning-free (excluding capability notes), got: {:?}",
        out.warnings
    );
    assert!(
        out.config_warnings.is_empty(),
        "healthy fixture should have no config-channel diagnostics either, got: {:?}",
        out.config_warnings
    );
}

#[test]
fn all_internal_tree_gets_the_dual_possibility_zero_symbols_warning() {
    // Real import edge (so the 0-dep-edges warning never fires), but neither file exports anything —
    // a legitimately all-internal tree, not a parser failure. The warning must still fire (coverage
    // self-report is wanted either way) but must present both possible causes rather than asserting
    // detection failed.
    let dir = TempDir::new("zzop-engine-diag-all-internal");
    dir.write(
        "a.ts",
        "import { b } from './b';\nfunction a() { return b(); }\n",
    );
    dir.write("b.ts", "function b() { return 1; }\n");

    let out = analyze_tree(dir.path(), &config());

    let w = out
        .warnings
        .iter()
        .find(|w| w.contains("0 exported symbols"))
        .unwrap_or_else(|| {
            panic!(
                "expected a 0-exported-symbols warning, got: {:?}",
                out.warnings
            )
        });
    assert!(
        w.contains("genuinely exports nothing"),
        "expected the dual-possibility phrasing, got: {w}"
    );
}

#[test]
fn git_disabled_run_emits_no_git_related_diagnostics_noise() {
    // Reuses the disconnected (0-edge, 0-symbol) tree so this test proves something the healthy-fixture
    // test above can't: even when OTHER diagnostics warnings legitimately fire, the git-window ones
    // (which would otherwise trigger on the honestly-zero total_changes/commits counts) never appear
    // when `EngineConfig::git` is `None` (the default — git was never attempted, not attempted-and-empty).
    let dir = disconnected_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.warnings.is_empty(),
        "expected the non-git diagnostics warnings to still fire"
    );
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("commit") || w.contains("git changes") || w.contains("submodule")),
        "git-disabled run must not emit git-window diagnostics noise, got: {:?}",
        out.warnings
    );
}

#[test]
fn typo_d_disabled_rules_entry_surfaces_a_self_report_warning() {
    // A `disabled_rules` entry that matches no known native-analysis id / pack id / "<pack>/<rule>" id
    // silently does nothing at the gating layer (`registry::is_enabled`'s exact-string-match contract) —
    // this proves the honest-output side: the diagnostics self-report must still tell the user their
    // typo'd entry had no effect (see `zzop_engine::analyze::unknown_disabled_rule_ids`).
    let dir = TempDir::new("zzop-engine-diag-unknown-disabled-rule");
    dir.write(
        "a.ts",
        "import { b } from './b';\nexport function a() { return b(); }\n",
    );
    dir.write("b.ts", "export function b() { return 1; }\n");

    let cfg = EngineConfig {
        source_id: "diagnostics-fixture".to_string(),
        rule_config: RuleConfig {
            disabled_rules: vec!["circular-typo".to_string()],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);

    // Config-channel diagnostic — rides `config_warnings`, not `warnings` (see
    // `zzop_engine::AnalyzeOutput::config_warnings`'s doc).
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("matching no known rule id")),
        "must NOT duplicate into warnings, got: {:?}",
        out.warnings
    );
    assert!(
        out.config_warnings
            .iter()
            .any(|w| w.contains("matching no known rule id") && w.contains("circular-typo")),
        "expected an unknown-disabled-rule-id self-report, got: {:?}",
        out.config_warnings
    );
}

#[test]
fn a_real_disabled_rules_entry_does_not_trigger_the_unknown_id_warning() {
    // Sanity check for the other direction: a real, known id (a native analysis id here) must never be
    // reported as unknown — the check is a set-membership diff, not a "any disabled_rules entry present"
    // trigger.
    let dir = TempDir::new("zzop-engine-diag-known-disabled-rule");
    dir.write(
        "a.ts",
        "import { b } from './b';\nexport function a() { return b(); }\n",
    );
    dir.write("b.ts", "export function b() { return 1; }\n");

    let cfg = EngineConfig {
        source_id: "diagnostics-fixture".to_string(),
        rule_config: RuleConfig {
            disabled_rules: vec!["circular".to_string()],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        !out.config_warnings
            .iter()
            .any(|w| w.contains("matching no known rule id")),
        "a real, known disabled_rules id must not be reported as unknown, got: {:?}",
        out.config_warnings
    );
}

// --- input-scope self-report (`input-scope-error`, disclosure registry: partial) --------------------

#[test]
fn nonexistent_root_self_reports_as_the_leading_warning() {
    // A typo'd root used to be silently absorbed as an empty tree (`files: 0` was the only trace).
    let cfg = config();
    let out = analyze_tree(Path::new("does/not/exist-anywhere-zzop"), &cfg);
    assert_eq!(out.file_count, 0);
    assert!(
        out.warnings
            .first()
            .is_some_and(|w| w.contains("does not exist or is not a directory")),
        "the scope warning must LEAD the warnings list (it qualifies every other line), got: {:?}",
        out.warnings
    );
    // The generic "root produced 0 analyzable files" line is redundant once the more specific
    // does-not-exist self-report already fired for this same root — must not appear at all.
    assert_eq!(
        out.warnings
            .iter()
            .filter(|w| w.contains("does not exist or is not a directory")
                || w.contains("root produced 0 analyzable files"))
            .count(),
        1,
        "expected exactly one scope-related warning (no generic duplicate), got: {:?}",
        out.warnings
    );
}

#[test]
fn existing_but_empty_root_self_reports_zero_source_files() {
    let dir = TempDir::new("zzop-engine-diag-empty-root");
    let out = analyze_tree(dir.path(), &config());
    assert_eq!(out.file_count, 0);
    assert!(
        out.warnings
            .first()
            .is_some_and(|w| w.contains("0 source files found under root")),
        "got: {:?}",
        out.warnings
    );
    // Unlike the nonexistent-root case, the root itself is valid here, so the generic
    // "root produced 0 analyzable files" line still carries information and must still fire.
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("root produced 0 analyzable files")),
        "the generic zero-files warning must still fire for a genuinely empty (but existing) root, got: {:?}",
        out.warnings
    );
}

#[test]
fn tree_with_source_files_emits_no_input_scope_warning() {
    let dir = TempDir::new("zzop-engine-diag-scoped-ok");
    dir.write("a.ts", "export function a() { return 1; }\n");
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !out.warnings.iter().any(|w| {
            w.contains("does not exist or is not a directory")
                || w.contains("0 source files found under root")
        }),
        "got: {:?}",
        out.warnings
    );
}
