//! End-to-end coverage for the two metrics channels wired into `AnalyzeOutput` by this task (the
//! co-churn/aggregates wiring follow-up to the crate-boundary split):
//!
//! - `folders` (`zpz_metrics::build_folder_aggregates`): NOT git-gated — populated on any tree that
//!   reaches assembly, since it only needs `nodes`/the dep graph (both built unconditionally).
//! - `layer_co_churn` (`zpz_metrics::build_cross_layer_co_churn` + `layer_of`): git-gated exactly like
//!   `scores`/`health` — `None` when `EngineConfig::git` is `None`, `Some` (real content) when git
//!   collection succeeds.
//!
//! Uses the same hand-rolled `TempDir` pattern as `packages/engine/tests/analyze_git.rs`/
//! `analyze_diagnostics.rs`; the git-fixture helpers below mirror `analyze_git.rs`'s own
//! `git_fixture_repo`/`git_available`/`run_git` (no shared test-support module exists in this workspace —
//! see those files' own doc comments for why each test file keeps its own copy).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use zpz_engine::{analyze_tree, EngineConfig, GitOptions};

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
        source_id: "channels-fixture".to_string(),
        ..EngineConfig::default()
    }
}

// ---------------------------------------------------------------------------------------------
// folders — not git-gated.
// ---------------------------------------------------------------------------------------------

/// Two files under `aaa/`, one under `bbb/`, with a cross-folder import edge `aaa -> bbb` — small enough
/// to hand-check `aggregate_by_folder`/`aggregate_dep_by_folder`'s output at `DEFAULT_FOLDER_DEPTH` (2,
/// which truncates to 1 leading segment here since neither path has 2 folder segments).
fn folder_fixture() -> TempDir {
    let dir = TempDir::new("zpz-engine-channels-folders");
    dir.write(
        "aaa/f1.ts",
        "import { x } from '../bbb/f3';\nexport const f1 = x;\n",
    );
    dir.write("aaa/f2.ts", "export const f2 = 1;\n");
    dir.write("bbb/f3.ts", "export const x = 1;\n");
    dir
}

#[test]
fn folders_present_and_deterministic_on_a_small_tree_without_git() {
    let dir = folder_fixture();
    let out = analyze_tree(dir.path(), &config());

    let folders = out.folders.expect("folders should be Some without git");
    let aaa = folders
        .summaries
        .iter()
        .find(|s| s.folder == "aaa")
        .expect("expected an \"aaa\" folder summary");
    assert_eq!(aaa.file_count, 2);
    let bbb = folders
        .summaries
        .iter()
        .find(|s| s.folder == "bbb")
        .expect("expected a \"bbb\" folder summary");
    assert_eq!(bbb.file_count, 1);

    assert_eq!(folders.edges.len(), 1);
    assert_eq!(folders.edges[0].source, "aaa");
    assert_eq!(folders.edges[0].target, "bbb");
    assert_eq!(folders.edges[0].count, 1);
}

#[test]
fn folders_are_byte_for_byte_identical_across_two_runs() {
    let dir = folder_fixture();
    let out1 = analyze_tree(dir.path(), &config());
    let out2 = analyze_tree(dir.path(), &config());
    assert_eq!(
        serde_json::to_value(&out1.folders).unwrap(),
        serde_json::to_value(&out2.folders).unwrap()
    );
}

#[test]
fn folders_present_even_on_a_git_disabled_empty_tree() {
    // Genuinely no dep edges, no folders beyond "." — folders must still be `Some` with empty/trivial
    // content, never `None` standing in for "ran and found nothing" (see `AnalyzeOutput::folders`'s doc).
    let dir = TempDir::new("zpz-engine-channels-folders-empty");
    dir.write("a.ts", "export const a = 1;\n");
    let out = analyze_tree(dir.path(), &config());
    let folders = out.folders.expect("folders should be Some even here");
    assert_eq!(folders.summaries.len(), 1);
    assert_eq!(folders.summaries[0].folder, ".");
    assert!(folders.edges.is_empty());
}

// ---------------------------------------------------------------------------------------------
// layer_co_churn — git-gated, mirrors scores/health.
// ---------------------------------------------------------------------------------------------

#[test]
fn layer_co_churn_is_none_when_git_is_disabled() {
    let dir = folder_fixture();
    let out = analyze_tree(dir.path(), &config());
    assert!(out.layer_co_churn.is_none());
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

/// A real git repo with `api/`- and `domains/`-layer files co-changed together across two commits (meets
/// `CrossLayerCoChurnOptions::default()`'s `min_co_changes: 2` threshold), plus a same-layer-only commit
/// that must NOT contribute a cross-layer pair.
fn git_layer_fixture_repo() -> TempDir {
    let dir = TempDir::new("zpz-engine-channels-git-fixture");
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test User"]);

    fs::create_dir_all(dir.path().join("api")).unwrap();
    fs::create_dir_all(dir.path().join("domains")).unwrap();
    fs::write(dir.path().join("api").join("x.ts"), "export const x = 1;\n").unwrap();
    fs::write(
        dir.path().join("domains").join("y.ts"),
        "export const y = 1;\n",
    )
    .unwrap();
    run_git(dir.path(), &["add", "api/x.ts", "domains/y.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FEAT] add x and y"]);

    fs::write(dir.path().join("api").join("x.ts"), "export const x = 2;\n").unwrap();
    fs::write(
        dir.path().join("domains").join("y.ts"),
        "export const y = 2;\n",
    )
    .unwrap();
    run_git(dir.path(), &["add", "api/x.ts", "domains/y.ts"]);
    run_git(
        dir.path(),
        &["commit", "-q", "-m", "[FIX] touch x and y together"],
    );

    // Same-layer-only commit: must not produce a cross-layer pair.
    fs::write(dir.path().join("api").join("z.ts"), "export const z = 1;\n").unwrap();
    run_git(dir.path(), &["add", "api/z.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FEAT] add z"]);

    dir
}

fn config_with_git() -> EngineConfig {
    EngineConfig {
        source_id: "channels-git-fixture".to_string(),
        git: Some(GitOptions::default()),
        ..EngineConfig::default()
    }
}

#[test]
fn layer_co_churn_reports_the_api_domains_cross_layer_pair_when_git_is_active() {
    if !git_available() {
        eprintln!(
            "skipping layer_co_churn_reports_the_api_domains_cross_layer_pair_when_git_is_active: git not on PATH"
        );
        return;
    }
    let dir = git_layer_fixture_repo();
    let out = analyze_tree(dir.path(), &config_with_git());

    let churn = out
        .layer_co_churn
        .expect("layer_co_churn should be Some when git is active");
    assert_eq!(
        churn.len(),
        1,
        "expected exactly one cross-layer pair: {churn:?}"
    );
    let pair = &churn[0];
    assert_eq!(pair.layer_a, "api");
    assert_eq!(pair.layer_b, "domains");
    // Two commits ("add x and y", "touch x and y together") each co-change the same file pair once.
    assert_eq!(pair.co_changes, 2);
    assert_eq!(pair.pairs, 1);
    assert_eq!(pair.examples[0].a, "api/x.ts");
    assert_eq!(pair.examples[0].b, "domains/y.ts");
    assert_eq!(pair.examples[0].count, 2);
}

#[test]
fn layer_co_churn_is_deterministic_across_two_runs() {
    if !git_available() {
        eprintln!("skipping layer_co_churn_is_deterministic_across_two_runs: git not on PATH");
        return;
    }
    let dir = git_layer_fixture_repo();
    let cfg = config_with_git();
    let out1 = analyze_tree(dir.path(), &cfg);
    let out2 = analyze_tree(dir.path(), &cfg);
    assert_eq!(
        serde_json::to_value(&out1.layer_co_churn).unwrap(),
        serde_json::to_value(&out2.layer_co_churn).unwrap()
    );
}
