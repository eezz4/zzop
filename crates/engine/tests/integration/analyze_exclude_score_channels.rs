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
//! ## The graph is not filtered — the SUBJECT SET is (revised 2026-07-30)
//! This file used to assert the opposite of what it asserts now, and the reason is worth keeping. The
//! original rule was "computation is never filtered, only emission": every `scores.*.score` and
//! `health.pain` were computed over the whole tree, so `exclude` moved the lists and left the numbers
//! exactly where they were. Measured on this repo, excluding three whole top-level directories replaced
//! every `criticalTop` slot and left `pain` identical to one decimal — the number stayed and the evidence
//! for it was deleted. For anyone excluding code they cannot change (vendored, generated), `pain` was a
//! figure with no available action behind it.
//!
//! What was right in the original rule is that DELETING the excluded file from the graph would corrupt
//! the metric: an excluded file is still a real importer and a real import target, and a scored file's
//! coupling and fan-out must not move because someone stopped reporting on its dependency. Both halves
//! are now true at once, because the filter attaches to the SUBJECT rather than to the graph — an
//! excluded file is not judged (it leaves the violation list AND the denominator, the same both-sides
//! rule `is_source` already enforced for `file_size_compliance`/`god_file`), while staying a full participant in every
//! other file's facts. `an_excluded_file_stays_a_real_import_target_for_the_files_that_import_it` pins
//! that second half.
//!
//! Two channels still take no filter of any kind:
//! - Slice/module-keyed rows (`cohesion.slices`, `sdp.violations`, `mainSequence.modules`, and the
//!   `modularity` rollup): their subject is a directory, not a file, so "this file is not judged" has no
//!   referent. Those four metrics never receive the subject gate either — the counted set and the
//!   printed set are kept identical on purpose.
//! - `warnings`, the config-diagnostics channel carrying the "your `exclude` is so broad the problem only
//!   LOOKS absent" tripwire. Filtering it would let the filter erase its own warning.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{GlobalExclude, RuleConfig};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig, GitOptions};
use zzop_metrics::Scores;
use zzop_test_support::skip_notice;

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
/// `scores.file_size_compliance.violations` and `scores.godFile.files` — two differently-shaped lists (`path`-keyed) off one
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
/// which puts it in the `file_size_compliance` and `godFile` violation lists. `legacy/legacy.rb` has no native parser, so
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
    for v in &s.feature_sliced_design.violations {
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
    out.extend(
        s.file_size_compliance
            .violations
            .iter()
            .map(|v| v.path.clone()),
    );
    out.extend(s.god_file.files.iter().map(|v| v.path.clone()));
    out.extend(s.rename_instability.files.iter().map(|v| v.path.clone()));
    out.extend(s.bus_factor.files.iter().map(|v| v.path.clone()));
    out
}

/// `architecture.criticalTop` is the FIRST list the summary prints. Under `exclude: ["legacy/**"]` it must
/// not name a `legacy/` path — and must still name the un-excluded hub, so the filter is shown to remove
/// rows rather than empty the channel.
#[test]
fn global_excludes_drop_critical_for_excluded_paths() {
    if !git_available() {
        skip_notice!("git not on PATH");
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
        skip_notice!("git not on PATH");
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
            .file_size_compliance
            .violations
            .iter()
            .any(|v| v.path == "legacy/hub.ts"),
        "fixture must produce a fileSizeCompliance violation for legacy/hub.ts: {:?}",
        base_scores.file_size_compliance.violations
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
            .file_size_compliance
            .violations
            .iter()
            .any(|v| v.path == "src/keep.ts")
            && scores
                .god_file
                .files
                .iter()
                .any(|v| v.path == "src/keep.ts"),
        "the exclude must REMOVE excluded rows, not empty the lists: fileSizeCompliance={:?} godFile={:?}",
        scores.file_size_compliance.violations,
        scores.god_file.files
    );
}

/// `exclude` means "do not JUDGE these paths", and until 2026-07-30 it reached only the lists — the score
/// and `pain` were computed over the whole tree and did not move. Measured on this repo, excluding three
/// whole top-level directories replaced every `criticalTop` slot and left `pain` identical to one decimal:
/// the number stayed and the evidence for it was deleted, which is the honesty defect inverted (the
/// figure was unactionable for anyone excluding code they cannot change).
///
/// The rule now: an excluded file is not a SUBJECT. It leaves both the violation list and the denominator
/// behind the score, exactly as a non-source file already did for `file_size_compliance`/`god_file` via `is_source`. Pinned
/// here as the seal on the reversal — this replaces the former
/// `global_excludes_do_not_change_pain_or_any_score`, whose name stated the invariant this test states the
/// negation of.
///
/// What did NOT change is pinned by its sibling below: the graph itself. An excluded file stays a node
/// with all its edges, so a scored file's coupling and fan-out are untouched.
#[test]
fn global_excludes_shrink_both_the_violation_list_and_its_denominator() {
    if !git_available() {
        skip_notice!("git not on PATH");
        return;
    }
    let dir = fixture_repo();
    let baseline = analyze_tree(dir.path(), &config_with_git());
    let out = analyze_tree(dir.path(), &config_excluding_legacy());

    let base_scores = baseline.scores.as_ref().expect("git-active run has scores");
    let scores = out.scores.as_ref().expect("git-active run has scores");

    // The denominator is the load-bearing half: leaving the excluded file in it while dropping it from the
    // violations would silently INFLATE the compliant ratio — a worse answer than either honest option.
    assert!(
        scores.file_size_compliance.total < base_scores.file_size_compliance.total,
        "the fileSizeCompliance denominator must shrink by the excluded file(s): baseline total={} excluded total={}",
        base_scores.file_size_compliance.total,
        scores.file_size_compliance.total
    );

    // And `pain` does NOT move here, which is the property worth pinning rather than an oversight: the
    // fixture mirrors `legacy/` and `src/` exactly, so excluding one half removes proportionally as much
    // from each numerator as from its denominator. A subject gate changes a RATIO; it does not subtract
    // badness. If this ever starts moving on a symmetric exclusion, the gate has become a graph edit.
    let base_health = baseline.health.as_ref().expect("git-active run has health");
    let health = out.health.as_ref().expect("git-active run has health");
    assert_eq!(
        base_health.pain, health.pain,
        "excluding a REPRESENTATIVE slice must leave pain where it is — the ratio is unchanged"
    );
}

/// The other direction, and the one the whole change exists for: excluding a slice that is WORSE than the
/// rest must lower `pain`. Here only `legacy/hub.ts` is excluded — the 320-line god file — while its three
/// small importers stay in the denominator, so the judged population gets proportionally cleaner.
///
/// Under the pre-2026-07-30 rule this assertion could not have been written at all: `pain` was computed
/// before any exclusion existed and was byte-identical for every `exclude` a user could spell.
#[test]
fn excluding_a_worse_than_average_file_lowers_pain() {
    if !git_available() {
        skip_notice!("git not on PATH");
        return;
    }
    let dir = fixture_repo();
    let baseline = analyze_tree(dir.path(), &config_with_git());
    let cfg = EngineConfig {
        rule_config: RuleConfig {
            global_excludes: vec![GlobalExclude {
                path: Some("legacy/hub.ts".to_string()),
                glob: None,
            }],
            ..RuleConfig::default()
        },
        ..config_with_git()
    };
    let out = analyze_tree(dir.path(), &cfg);

    let base_health = baseline.health.as_ref().expect("git-active run has health");
    let health = out.health.as_ref().expect("git-active run has health");
    assert!(
        health.pain < base_health.pain,
        "excluding a worse-than-average file must lower pain: baseline={:?} excluded={:?}",
        base_health.pain,
        health.pain
    );
}

/// The other half of the same rule, and the reason this is a subject gate rather than a graph edit: an
/// excluded file is still a real node with real edges. A scored file that imports excluded code keeps that
/// dependency — its `fanOut` is unchanged — because the dependency exists and the SCORED file is the one
/// that chose it. Deleting the node instead would not filter the report, it would report that a real
/// import does not exist, and would make every importer look cleaner than it is.
#[test]
fn an_excluded_file_stays_a_real_import_target_for_the_files_that_import_it() {
    if !git_available() {
        skip_notice!("git not on PATH");
        return;
    }
    let dir = fixture_repo();
    let baseline = analyze_tree(dir.path(), &config_with_git());
    let out = analyze_tree(dir.path(), &config_excluding_legacy());

    let fan_out =
        |o: &AnalyzeOutput, id: &str| o.nodes.iter().find(|n| n.id == id).map(|n| n.fan_out);

    for node in &baseline.nodes {
        assert_eq!(
            fan_out(&baseline, &node.id),
            fan_out(&out, &node.id),
            "`exclude` must not edit the graph — {} lost or gained fan-out",
            node.id
        );
    }
}

/// `warnings` is the config-diagnostics channel: `exclude` set so broad that a real problem only LOOKS
/// absent is disclosed THERE. Filtering it would let the filter erase its own warning, so the excluded run's
/// warnings must be byte-identical to the baseline's — including the per-extension disclosure that names
/// `legacy/legacy.rb`, a path the config excludes.
#[test]
fn global_excludes_do_not_reach_warnings() {
    if !git_available() {
        skip_notice!("git not on PATH");
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
    // Containment, not equality: the contract is that `exclude` never REMOVES a warning, and the excluded
    // run legitimately gains one the baseline cannot have — the scoping disclosure, which exists precisely
    // because `exclude` now moves `pain` (`scoring_scope_warning`). Asserting equality would have made
    // adding any honest disclosure look like a regression.
    for w in &baseline.warnings {
        assert!(
            out.warnings.contains(w),
            "`exclude` must not erase a warning — missing from the excluded run: {w}"
        );
    }
    assert!(
        out.warnings.iter().any(|w| w.contains("from SCORING")),
        "the excluded run must disclose that `exclude` changed the scored population: {:?}",
        out.warnings
    );
    assert!(
        !baseline.warnings.iter().any(|w| w.contains("from SCORING")),
        "a run with no `exclude` has no scoping to disclose: {:?}",
        baseline.warnings
    );
}
