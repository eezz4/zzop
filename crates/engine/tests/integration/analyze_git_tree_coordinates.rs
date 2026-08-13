//! The git-coordinate gate: **every git-derived path an analyzed tree publishes is in that tree's own
//! coordinate system.**
//!
//! Why this file exists. `zzop_git` collects history for the whole REPOSITORY, keyed and reported
//! relative to the repository root — deliberately, because `crates/git/src/process.rs` pins
//! `diff.relative=false` so one run's trees can share a single collection memo. Everything else a tree
//! publishes (`nodes`, `folders`, `dep`, and therefore `coChange`/`layerCoChurn`) is relative to the
//! ANALYZED TREE root. When the two roots are the same directory the coordinate systems coincide and
//! nobody notices. When the analyzed tree is a SUBDIRECTORY of its repository — the `zzop cross ./fe
//! ./be` monorepo shape the cross-layer join exists for — they diverge, and nothing downstream can
//! detect it: a repo-relative path is still a well-formed path, so `layer_of` classifies it happily
//! and `co_change` emits confident pairs over files the analyzed tree does not contain.
//!
//! Measured on this repository on 2026-08-13, before the fix: analyzing `cases/trees/api-be` (a 24-file
//! TypeScript fixture) produced 1903 co-change edges over 317 distinct paths, **zero** of which were
//! inside the tree; the edge list was byte-identical to the one the repository root produces
//! (`zzop graph --domain cochange --top 5000 cases/trees/api-be` vs `... .`), and every `nodes[]` entry
//! carried `changeCount: 0`. A populated-but-wrong channel is worse than an empty one, because `None`
//! is already this codebase's word for "not measured" and a filled list revokes it.
//!
//! What this file pins is the CONTRACT, not the repository: a subtree's git-derived output names only
//! its own files, and the sibling subtree in the same run gets a different answer.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use zzop_engine::{analyze_trees, AnalyzeOutput, EngineConfig, GitOptions};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
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

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

/// One repository, two package subtrees, and a root-level file none of them contains.
///
/// `alpha` holds two files in two different top-level folders (`routes/`, `services/`) so a co-change
/// between them is also a CROSS-LAYER co-change under `zzop_metrics::layer_of` — that is what makes one
/// fixture exercise `coChange` and `layerCoChurn` together. The commits are shaped so that `alpha` has
/// two in-tree co-changes (`layerCoChurn`'s `MIN_CO_CHANGES` is 2) and `beta` has none at all, which is
/// the second half of the contract: two trees of one run must not receive the same answer.
fn monorepo() -> TempDir {
    let dir = TempDir::new("zzop-git-tree-coordinates");
    let root = dir.path();
    run_git(root, &["init", "-q"]);
    run_git(root, &["config", "user.email", "test@example.com"]);
    run_git(root, &["config", "user.name", "Test User"]);

    write(root, "README.md", "# repo\n");
    write(
        root,
        "packages/alpha/routes/handler.ts",
        "import { load } from '../services/store';\nexport const handler = () => load();\n",
    );
    write(
        root,
        "packages/alpha/services/store.ts",
        "export const load = () => 1;\n",
    );
    write(
        root,
        "packages/beta/index.ts",
        "export const beta = () => 2;\n",
    );
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-q", "-m", "[FEAT] two packages"]);

    // A second co-change between alpha's two layers — `layerCoChurn` drops pairs below 2.
    write(
        root,
        "packages/alpha/routes/handler.ts",
        "import { load } from '../services/store';\nexport const handler = () => load() + 1;\n",
    );
    write(
        root,
        "packages/alpha/services/store.ts",
        "export const load = () => 3;\n",
    );
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-q", "-m", "[FIX] alpha both layers"]);

    // A commit alpha must never hear about: it touches only beta and the repository root.
    write(root, "README.md", "# repo\n\nmore\n");
    write(
        root,
        "packages/beta/index.ts",
        "export const beta = () => 4;\n",
    );
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-q", "-m", "[DOCS] beta and readme"]);

    dir
}

fn analyze(repo: &Path, subtrees: &[&str]) -> Vec<(String, AnalyzeOutput)> {
    let trees: Vec<(PathBuf, EngineConfig)> = subtrees
        .iter()
        .map(|rel| {
            let root = if rel.is_empty() {
                repo.to_path_buf()
            } else {
                repo.join(rel)
            };
            (
                root,
                EngineConfig {
                    source_id: (*rel).to_string(),
                    git: Some(GitOptions::default()),
                    ..EngineConfig::default()
                },
            )
        })
        .collect();
    analyze_trees(&trees)
        .trees
        .into_iter()
        .map(|(_, source_id, output)| (source_id, output))
        .collect()
}

/// Every path named by a tree's git-derived channels, in one set — the thing that must be expressible
/// in the tree's own coordinates.
fn git_derived_paths(out: &AnalyzeOutput) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for edge in out.co_change.as_ref().expect("git was active: co_change") {
        paths.insert(edge.a.clone());
        paths.insert(edge.b.clone());
    }
    for churn in out
        .layer_co_churn
        .as_ref()
        .expect("git was active: layer_co_churn")
    {
        for ex in &churn.examples {
            paths.insert(ex.a.clone());
            paths.insert(ex.b.clone());
        }
    }
    paths
}

#[test]
fn a_subtrees_co_change_names_only_files_that_subtree_contains() {
    if !git_available() {
        zzop_test_support::skip_notice!("git not on PATH");
        return;
    }
    let repo = monorepo();
    let out = analyze(repo.path(), &["packages/alpha"]);
    let (_, alpha) = &out[0];

    let named = git_derived_paths(alpha);
    assert!(
        !named.is_empty(),
        "precondition: alpha's history must produce co-change at all, else this test proves nothing"
    );
    for path in &named {
        assert!(
            repo.path().join("packages/alpha").join(path).is_file(),
            "git-derived channels named `{path}`, which is not a file of the analyzed tree — the \
             commit paths are still in the REPOSITORY's coordinate system. All named: {named:?}"
        );
    }
    assert_eq!(
        named,
        BTreeSet::from([
            "routes/handler.ts".to_string(),
            "services/store.ts".to_string()
        ]),
        "alpha's own two files, tree-relative, and nothing from beta or the repository root"
    );
}

#[test]
fn a_subtrees_nodes_carry_the_change_counts_its_own_history_earned() {
    if !git_available() {
        zzop_test_support::skip_notice!("git not on PATH");
        return;
    }
    let repo = monorepo();
    let out = analyze(repo.path(), &["packages/alpha"]);
    let (_, alpha) = &out[0];

    for id in ["routes/handler.ts", "services/store.ts"] {
        let node = alpha
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("node `{id}` missing; nodes: {:?}", ids(alpha)));
        assert_eq!(
            node.change_count, 2,
            "`{id}` was committed twice, but its node reports changeCount {} — the git stats are \
             keyed by repository-relative path and this tree's node ids are tree-relative, so the \
             join silently misses and every file reads as never-changed",
            node.change_count
        );
        assert!(
            node.churn > 0,
            "`{id}` has real line churn in history, but its node reports {}",
            node.churn
        );
    }
}

#[test]
fn two_subtrees_of_one_run_get_their_own_answers_not_the_repositorys() {
    if !git_available() {
        zzop_test_support::skip_notice!("git not on PATH");
        return;
    }
    let repo = monorepo();
    let out = analyze(repo.path(), &["packages/alpha", "packages/beta"]);
    let alpha = &out[0].1;
    let beta = &out[1].1;

    assert_ne!(
        git_derived_paths(alpha),
        git_derived_paths(beta),
        "both trees resolve to one `.git` and share one collection memo; sharing the COLLECTION must \
         not mean sharing the ANSWER"
    );
    // beta is a single file: every commit that touched it touched exactly one beta file, and a
    // one-file commit couples nothing. Measured and empty, which is what `Some(vec![])` means — as
    // distinct from the `None` a git-less run produces.
    assert_eq!(
        beta.co_change.as_deref(),
        Some(&[][..]),
        "beta's own history forms no pair, and saying so is different from saying nothing was measured"
    );
    assert_eq!(
        beta.layer_co_churn.as_deref(),
        Some(&[][..]),
        "same for the layer view"
    );
    let beta_index = beta
        .nodes
        .iter()
        .find(|n| n.id == "index.ts")
        .expect("beta's only file must be a node");
    assert_eq!(
        beta_index.change_count, 2,
        "beta's file was committed twice; an empty co_change must come from the coupling window, \
         never from history that failed to join"
    );
}

/// The identity half of the contract: when the analyzed tree IS the repository root there is nothing to
/// rebase, and the answer must be exactly what it always was — every path repo-relative, the root file
/// and both packages present.
#[test]
fn a_tree_that_is_its_own_repository_root_is_left_alone() {
    if !git_available() {
        zzop_test_support::skip_notice!("git not on PATH");
        return;
    }
    let repo = monorepo();
    let out = analyze(repo.path(), &[""]);
    let (_, root) = &out[0];

    let named = git_derived_paths(root);
    assert!(
        named.contains("packages/alpha/routes/handler.ts"),
        "the repository root must still see full repo-relative paths: {named:?}"
    );
    assert!(
        named.contains("packages/beta/index.ts") || named.contains("README.md"),
        "the root's own third commit couples beta with README; neither may be filtered away: {named:?}"
    );
    let node = root
        .nodes
        .iter()
        .find(|n| n.id == "packages/alpha/routes/handler.ts")
        .expect("root-level node ids are repo-relative");
    assert_eq!(node.change_count, 2);
}

fn ids(out: &AnalyzeOutput) -> Vec<&str> {
    out.nodes.iter().map(|n| n.id.as_str()).collect()
}
