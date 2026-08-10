//! The git-process census gate: **one run over N trees sharing one repository spawns git once.**
//!
//! Why this file exists at all. `collect_git` runs per TREE, and in a monorepo every tree resolves to
//! the same `.git`; `zzop_git` does no path or branch scoping, so those calls all spawn the same
//! `git log --numstat` and get byte-identical output back. Measured 2026-08-07 on a 22-tree monorepo,
//! that repetition was **90.9%** of the warm wall clock — and CI has no perf job, so nobody could say
//! when the regression arrived. `zzop_engine::analyze_trees` now shares one memo across the run; this
//! is the gate that keeps it shared.
//!
//! Why it asserts an EQUALITY and not a duration. `spawn_log().len() == unique repo roots` is a
//! statement about cache EXISTENCE, so it cannot flake on a slow machine and it invents no
//! hand-picked threshold — the two things that disqualify every timing-shaped perf test here.
//!
//! Why this test is ALONE in its own `tests/*.rs`. The counter behind `zzop_git::spawn_log` is
//! process-global and append-only, and `cargo` runs the tests within one file concurrently — a second
//! test in this binary would contribute spawns to the same count and make the equality meaningless.
//! Same constraint and same reason as `crates/engine/tests/analyze_parse_census.rs`.
//!
//! Why a missing `git` makes this RED rather than skipped. Every other git test here skips when git
//! is unavailable, which is right for them: they test what git returns. This one tests how many times
//! we CALL it, and a skip would report green for a gate that measured nothing. So the absence of git
//! is a broken harness, and it says so.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use zzop_engine::{analyze_trees, EngineConfig, GitOptions};

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

/// One repository holding three sibling package directories — the shape of every monorepo this gate
/// is about. The harness's own `git init`/`commit` calls are `std::process::Command` right here and
/// never reach `zzop_git::spawn_git`, so they cannot pollute the census (this is also why shimming
/// `git` on `PATH` was rejected as the measuring technique: it would have counted them).
fn monorepo_with_three_trees() -> TempDir {
    let dir = TempDir::new("zzop-git-spawn-census");
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test User"]);
    for pkg in ["alpha", "beta", "gamma"] {
        let sub = dir.path().join("packages").join(pkg);
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            sub.join("index.ts"),
            format!("export function {pkg}() {{ return 1; }}\n"),
        )
        .unwrap();
    }
    run_git(dir.path(), &["add", "."]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FEAT] three packages"]);
    dir
}

#[test]
fn three_trees_in_one_repository_collect_git_history_exactly_once() {
    assert!(
        Command::new("git").arg("--version").output().is_ok(),
        "`git` is unavailable, and this gate must NOT skip: it counts git spawns, so skipping would \
         report green for a measurement that never happened. Install git or exclude this test \
         explicitly — do not let it pass silently."
    );

    let repo = monorepo_with_three_trees();
    let trees: Vec<(PathBuf, EngineConfig)> = ["alpha", "beta", "gamma"]
        .iter()
        .map(|pkg| {
            (
                repo.path().join("packages").join(pkg),
                EngineConfig {
                    source_id: (*pkg).to_string(),
                    git: Some(GitOptions::default()),
                    ..EngineConfig::default()
                },
            )
        })
        .collect();

    let before = zzop_git::spawn_log().len();
    let out = analyze_trees(&trees);
    let spawned = zzop_git::spawn_log().len() - before;

    // Precondition: git collection actually RAN. Without this the equality below would also be
    // satisfied by a run that collected nothing at all — the classic way a cache gate passes for the
    // wrong reason. `nodes` carry git-derived change counts, so a non-empty tree output with git
    // active is the observable proof.
    assert_eq!(out.trees.len(), 3, "all three trees must be analyzed");
    assert!(
        spawned >= 1,
        "git collection never ran — the census measured nothing, so its equality proves nothing"
    );

    // THE gate. Three trees, one `.git`, one collection. `zzop_git::repo_root` resolves all three
    // tree roots to the same repository directory in-process (no `git rev-parse` — computing the
    // denominator must not spend the process being counted), so the memo key is shared and the second
    // and third trees hit it.
    assert_eq!(
        spawned,
        1,
        "3 trees sharing one repository must collect git history ONCE, not once per tree; spawned \
         in: {:?}",
        zzop_git::spawn_log()
    );
}
