//! The top-level `exclude` config key (`RuleConfig::global_excludes`) must reach the two REMAINING
//! report channels that once ignored it: `critical` (the summary's `architecture.criticalTop`) and the
//! per-metric violation lists under `scores.*`.
//!
//! ## Why this test exists
//! `exclude` is one user statement — "do not report on these paths" — and it already reached `findings`,
//! `recommendations`, and `crossLayerFindings`. It did NOT reach `critical` or `scores.*`, so two fields
//! inside the SAME `architecture` object answered in opposite directions: measured on this repo with
//! `exclude: ["crates/core/**"]`, all three `criticalTop` slots were `crates/core/...` while
//! `topRecommendation` in the same run honoured the exclusion. A run whose headline names a path its own
//! config excluded contradicts that config; see `zzop_metrics::recommendations`' module doc for the
//! recommendations half of the same defect.
//!
//! ## Computation is NOT filtered — only emission
//! Blast radius, every `scores.*.score`, and `health.pain` are computed over the WHOLE graph: an excluded
//! file is still a real importer, and pretending otherwise would corrupt the metric rather than filter the
//! report. The filter attaches at the ranking/emission step only. The two `unchanged` tests below pin the
//! directions a later "cleanup" would break:
//! - `pain` is a whole-tree rollup; filtering it would make the number incomparable with any other run.
//! - `warnings` is the config-diagnostics channel that carries the "your `exclude` is so broad the problem
//!   only LOOKS absent" tripwire. Filtering it would let the filter erase its own warning.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{GlobalExclude, RuleConfig};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig, GitOptions};
use zzop_metrics::Scores;

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

/// `DEFAULT_LOC_LIMIT` is 150 and `god_file` fires at 2x that, so a 320-line file lands in BOTH
/// `scores.sfc.violations` and `scores.godFile.files` — two differently-shaped lists (`path`-keyed) off one
/// fixture file.
fn big_module(name: &str) -> String {
    let mut s = format!("export function {name}() {{ return 1; }}\n");
    for i in 0..320 {
        s.push_str(&format!("export const {name}_{i} = {i};\n"));
    }
    s
}

/// Two mirrored hub trees, one under `legacy/` (the path the test excludes) and one under `src/` (the
/// control that must survive the exclusion).
///
/// Each hub is imported by three siblings, so its transitive blast radius is 3 —
/// `CRITICALITY_MIN_BLAST_RADIUS`, the gate for landing in `critical` at all. Each hub is also 320 lines,
/// which puts it in the `sfc` and `godFile` violation lists. `legacy/legacy.rb` has no native parser, so
/// the per-extension "bring an adapter" warning NAMES an excluded path — that is the `warnings` tripwire
/// this filter must not erase.
fn fixture_repo() -> TempDir {
    let dir = TempDir::new("zzop-engine-exclude-score-channels");
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test User"]);

    for (root, hub) in [("legacy", "hub"), ("src", "keep")] {
        dir.write(&format!("{root}/{hub}.ts"), &big_module(hub));
        for importer in ["one", "two", "three"] {
            dir.write(
                &format!("{root}/{importer}.ts"),
                &format!(
                    "import {{ {hub} }} from './{hub}';\nexport function {importer}() {{ return {hub}(); }}\n"
                ),
            );
        }
    }
    dir.write("legacy/legacy.rb", "def legacy\n  1\nend\n");

    run_git(dir.path(), &["add", "."]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FEAT] initial tree"]);

    // A second commit so every node carries a real `changeCount` and the git-derived channels populate.
    dir.write("src/keep.ts", &format!("{}\n", big_module("keep")));
    dir.write("legacy/hub.ts", &format!("{}\n", big_module("hub")));
    run_git(dir.path(), &["add", "."]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FIX] touch both hubs"]);

    dir
}

fn config_with_git() -> EngineConfig {
    EngineConfig {
        source_id: "exclude-score-channels-fixture".to_string(),
        git: Some(GitOptions::default()),
        ..EngineConfig::default()
    }
}

fn config_excluding_legacy() -> EngineConfig {
    EngineConfig {
        rule_config: RuleConfig {
            global_excludes: vec![GlobalExclude {
                path: None,
                glob: Some("legacy/**".to_string()),
            }],
            ..RuleConfig::default()
        },
        ..config_with_git()
    }
}

fn critical_paths(out: &AnalyzeOutput) -> Vec<String> {
    out.critical.iter().map(|c| c.path.clone()).collect()
}

/// Every FILE PATH any `scores.*` violation/detail list names, flattened.
///
/// Slice- and module-keyed rows (`cohesion.slices`, `sdp.violations`, `mainSequence.modules`) are
/// deliberately absent: their keys are directory/slice identifiers, not file paths, and each row is itself
/// a whole-directory rollup — the same reason `pain` is not filtered.
fn scores_paths(s: &Scores) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in &s.fsd.violations {
        out.push(v.from.clone());
        out.push(v.to.clone());
    }
    for v in &s.hierarchy.violations {
        out.push(v.from.clone());
        out.push(v.to.clone());
    }
    for v in &s.public_api.deep_imports {
        out.push(v.from.clone());
        out.push(v.to.clone());
    }
    for v in &s.sibling_cross.violations {
        out.push(v.from.clone());
        out.push(v.to.clone());
    }
    for p in &s.diamond.pairs {
        out.push(p.root.clone());
        out.push(p.leaf.clone());
        out.extend(p.through.iter().cloned());
    }
    out.extend(s.sfc.violations.iter().map(|v| v.path.clone()));
    out.extend(s.god_file.files.iter().map(|v| v.path.clone()));
    out.extend(s.rename_instability.files.iter().map(|v| v.path.clone()));
    out.extend(s.bus_factor.files.iter().map(|v| v.path.clone()));
    out.extend(s.type_safety.violations.iter().map(|v| v.path.clone()));
    out.extend(s.lod.violations.iter().map(|v| v.path.clone()));
    out
}

/// `architecture.criticalTop` is the FIRST list the summary prints. Under `exclude: ["legacy/**"]` it must
/// not name a `legacy/` path — and must still name the un-excluded hub, so the filter is shown to remove
/// rows rather than empty the channel.
#[test]
fn global_excludes_drop_critical_for_excluded_paths() {
    if !git_available() {
        eprintln!("skipping global_excludes_drop_critical_for_excluded_paths: git not on PATH");
        return;
    }
    let dir = fixture_repo();

    let baseline = analyze_tree(dir.path(), &config_with_git());
    assert!(
        critical_paths(&baseline)
            .iter()
            .any(|p| p == "legacy/hub.ts"),
        "fixture must put legacy/hub.ts in `critical`, else the exclude assertion is vacuous: {:?}",
        critical_paths(&baseline)
    );
    assert!(
        critical_paths(&baseline).iter().any(|p| p == "src/keep.ts"),
        "fixture must also put the un-excluded src/keep.ts in `critical`, else the survivor assertion is vacuous: {:?}",
        critical_paths(&baseline)
    );

    let out = analyze_tree(dir.path(), &config_excluding_legacy());
    assert!(
        !critical_paths(&out)
            .iter()
            .any(|p| p.starts_with("legacy/")),
        "a top-level `exclude` must drop the path from `critical`/criticalTop too: {:?}",
        critical_paths(&out)
    );
    assert!(
        critical_paths(&out).iter().any(|p| p == "src/keep.ts"),
        "the exclude must REMOVE excluded rows, not empty the channel: {:?}",
        critical_paths(&out)
    );
}

/// The per-metric violation lists under `scores.*` are the same reporting surface as `criticalTop`, just
/// deeper in the object.
#[test]
fn global_excludes_drop_scores_violation_lists_for_excluded_paths() {
    if !git_available() {
        eprintln!(
            "skipping global_excludes_drop_scores_violation_lists_for_excluded_paths: git not on PATH"
        );
        return;
    }
    let dir = fixture_repo();

    let baseline = analyze_tree(dir.path(), &config_with_git());
    let base_scores = baseline.scores.as_ref().expect("git-active run has scores");
    assert!(
        scores_paths(base_scores).iter().any(|p| p == "legacy/hub.ts"),
        "fixture must name legacy/hub.ts in at least one scores list, else the assertion is vacuous: {:?}",
        scores_paths(base_scores)
    );
    assert!(
        base_scores
            .sfc
            .violations
            .iter()
            .any(|v| v.path == "legacy/hub.ts"),
        "fixture must produce an sfc violation for legacy/hub.ts: {:?}",
        base_scores.sfc.violations
    );
    assert!(
        base_scores
            .god_file
            .files
            .iter()
            .any(|v| v.path == "legacy/hub.ts"),
        "fixture must produce a godFile row for legacy/hub.ts: {:?}",
        base_scores.god_file.files
    );

    let out = analyze_tree(dir.path(), &config_excluding_legacy());
    let scores = out.scores.as_ref().expect("git-active run has scores");
    let named = scores_paths(scores);
    assert!(
        !named.iter().any(|p| p.starts_with("legacy/")),
        "a top-level `exclude` must drop the path from every scores.* violation list: {:?}",
        named
    );
    assert!(
        scores
            .sfc
            .violations
            .iter()
            .any(|v| v.path == "src/keep.ts")
            && scores
                .god_file
                .files
                .iter()
                .any(|v| v.path == "src/keep.ts"),
        "the exclude must REMOVE excluded rows, not empty the lists: sfc={:?} godFile={:?}",
        scores.sfc.violations,
        scores.god_file.files
    );
}

/// `pain` is a whole-tree rollup — filtering it would make the number incomparable with any other run, so
/// the emission filter must run AFTER `compute_health_index`. Every `scores.*.score` (and the `sfc`
/// compliant/total denominator the score is derived from) is the same kind of rollup and is pinned here
/// too: the score says how the tree is, the list says what to look at.
#[test]
fn global_excludes_do_not_change_pain_or_any_score() {
    if !git_available() {
        eprintln!("skipping global_excludes_do_not_change_pain_or_any_score: git not on PATH");
        return;
    }
    let dir = fixture_repo();
    let baseline = analyze_tree(dir.path(), &config_with_git());
    let out = analyze_tree(dir.path(), &config_excluding_legacy());

    let base_health = baseline.health.as_ref().expect("git-active run has health");
    let health = out.health.as_ref().expect("git-active run has health");
    assert_eq!(
        base_health, health,
        "pain (and its contributors) is a whole-tree rollup — `exclude` must not move it"
    );

    let base_scores = baseline.scores.as_ref().expect("git-active run has scores");
    let scores = out.scores.as_ref().expect("git-active run has scores");
    assert_eq!(
        base_scores.sfc.score, scores.sfc.score,
        "sfc.score is computed over the whole tree; only its `violations` list is filtered"
    );
    assert_eq!(
        (base_scores.sfc.compliant, base_scores.sfc.total),
        (scores.sfc.compliant, scores.sfc.total),
        "the sfc denominator must not shrink — that would be filtering the COMPUTATION"
    );
    assert_eq!(
        base_scores.god_file.score, scores.god_file.score,
        "godFile.score is computed over the whole tree; only its `files` list is filtered"
    );
    assert_eq!(
        base_scores.fsd.score, scores.fsd.score,
        "fsd.score is computed over the whole tree; only its `violations` list is filtered"
    );
}

/// `warnings` is the config-diagnostics channel: `exclude` set so broad that a real problem only LOOKS
/// absent is disclosed THERE. Filtering it would let the filter erase its own warning, so the excluded run's
/// warnings must be byte-identical to the baseline's — including the per-extension disclosure that names
/// `legacy/legacy.rb`, a path the config excludes.
#[test]
fn global_excludes_do_not_reach_warnings() {
    if !git_available() {
        eprintln!("skipping global_excludes_do_not_reach_warnings: git not on PATH");
        return;
    }
    let dir = fixture_repo();
    let baseline = analyze_tree(dir.path(), &config_with_git());
    let out = analyze_tree(dir.path(), &config_excluding_legacy());

    assert!(
        baseline
            .warnings
            .iter()
            .any(|w| w.contains("legacy/legacy.rb")),
        "fixture must produce a warning naming an excluded path, else the assertion is vacuous: {:?}",
        baseline.warnings
    );
    assert_eq!(
        baseline.warnings, out.warnings,
        "`exclude` must not reach the warnings channel — it would erase its own over-broad-exclude tripwire"
    );
}
