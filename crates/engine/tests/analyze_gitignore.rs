//! Exercises `pipeline::walk_files` end to end via `analyze_tree`: the walker must honor COMMITTED
//! `.gitignore` files (nested ones included, real negation semantics, AND ancestor ones above the analysis
//! root up to the git toplevel — see tests 5-8 below) while leaving every other current behavior —
//! `DEFAULT_SKIP_DIRS`, non-git trees, deterministic file ordering/count — unchanged. See
//! `crates/engine/src/pipeline.rs::walk_files`'s doc (and `ancestor_gitignores`/`ancestor_ignored`'s) for
//! the exact `ignore::WalkBuilder` flag choices and ancestor-climb mechanism this file is a regression guard
//! for.
//!
//! `AnalyzeOutput::ir.ir.loc` (a `rel path -> loc` map, populated once per walked file regardless of
//! language — see `analyze.rs`'s `loc_by_path`) is used throughout as the ground truth for "which files did
//! the walk actually visit": it is a stronger, more direct signal than `findings` (this crate loads zero
//! rule packs below, so findings are always empty either way) or `ir.ir.symbols` (only populated for files
//! with top-level declarations — a `.gitignore` file itself, or an empty file, would show up in `loc` but
//! never in `symbols`).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, AnalyzeOutput, DispatchConfig, EngineConfig, DEFAULT_SIZE_CAP};

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
        source_id: "gitignore-fixture".to_string(),
        dispatch: DispatchConfig::default(),
        size_cap: DEFAULT_SIZE_CAP,
        rule_config: Default::default(),
        packs: Vec::new(),
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

/// Like `scan`, but the analysis root is `dir.path().join(sub)` instead of `dir.path()` itself — the
/// ancestor-`.gitignore` fixtures below need a root strictly BELOW the fixture's `.git` toplevel.
fn scan_root(dir: &TempDir, sub: &str) -> AnalyzeOutput {
    analyze_tree(&dir.path().join(sub), &config())
}

/// Every rel path the walk actually visited, per `AnalyzeOutput::ir.ir.loc`'s keys (see module doc) — one
/// entry per walked file regardless of language/degradation, which `findings`/`symbols` do not guarantee.
fn walked_files(out: &AnalyzeOutput) -> BTreeSet<String> {
    out.ir.ir.loc.keys().cloned().collect()
}

/// Every rel path referenced by a projected `SourceSymbol` — used alongside `walked_files` where the task
/// spec asks for "no finding/symbol references them" (findings are trivially empty here: `config()` loads
/// zero rule packs).
fn symbol_files(out: &AnalyzeOutput) -> BTreeSet<String> {
    out.ir.ir.symbols.iter().map(|s| s.file.clone()).collect()
}

// --- 1. root .gitignore + nested sub/.gitignore ---

#[test]
fn gitignored_files_at_root_and_nested_are_excluded_non_ignored_siblings_are_kept() {
    let dir = TempDir::new("zzop-gi-nested");
    dir.write(".gitignore", "out/\n");
    dir.write("keep.ts", "export function keep() { return 1; }\n");
    dir.write(
        "out/generated.ts",
        "export function generated() { return 2; }\n",
    );
    dir.write("sub/.gitignore", "local.ts\n");
    dir.write("sub/keep2.ts", "export function keep2() { return 3; }\n");
    dir.write("sub/local.ts", "export function local() { return 4; }\n");

    let out = scan(&dir);
    let walked = walked_files(&out);
    let symbols = symbol_files(&out);

    assert!(walked.contains("keep.ts"), "{walked:?}");
    assert!(walked.contains("sub/keep2.ts"), "{walked:?}");
    assert!(
        !walked.contains("out/generated.ts"),
        "out/ is gitignored at the root -> generated.ts must never be walked: {walked:?}"
    );
    assert!(
        !walked.contains("sub/local.ts"),
        "sub/local.ts is gitignored by the NESTED sub/.gitignore -> must never be walked: {walked:?}"
    );
    assert!(!symbols.contains("out/generated.ts"), "{symbols:?}");
    assert!(!symbols.contains("sub/local.ts"), "{symbols:?}");
    assert!(symbols.contains("keep.ts"), "{symbols:?}");
    assert!(symbols.contains("sub/keep2.ts"), "{symbols:?}");

    // Exact accounting: keep.ts, sub/keep2.ts, and the two .gitignore files themselves (never gitignored,
    // always tracked, always walked as ordinary lexical-only files) — out/generated.ts and sub/local.ts
    // excluded.
    assert_eq!(out.file_count, 4, "{walked:?}");
    assert_eq!(walked.len(), 4, "{walked:?}");
}

// --- 2. no .gitignore anywhere -> identical file_count to a same-shape control tree (regression guard) ---

#[test]
fn tree_without_any_gitignore_walks_every_file_same_as_before() {
    let dir = TempDir::new("zzop-gi-none");
    dir.write("a.ts", "export function a() { return 1; }\n");
    dir.write("b.ts", "export function b() { return 2; }\n");
    dir.write("sub/c.ts", "export function c() { return 3; }\n");
    dir.write("sub/deep/d.ts", "export function d() { return 4; }\n");

    let out = scan(&dir);
    let walked = walked_files(&out);

    for f in ["a.ts", "b.ts", "sub/c.ts", "sub/deep/d.ts"] {
        assert!(walked.contains(f), "{walked:?}");
    }
    assert_eq!(out.file_count, 4, "{walked:?}");
    assert_eq!(walked.len(), 4, "{walked:?}");
}

// --- 3. real gitignore negation semantics ---

#[test]
fn negated_pattern_under_an_otherwise_ignored_glob_is_still_analyzed() {
    // `sub2/*` (a glob over sub2's CHILDREN, not a directory-match pattern like the bare `sub2/` would be)
    // ignores every direct child of `sub2/` except `keep.ts`, which `!sub2/keep.ts` re-includes. This is the
    // real-gitignore-engine case a naive independent-per-line matcher gets wrong: `sub2/*` alone would also
    // match `keep.ts`, so the negation line must be applied AFTER (in file order) to correctly re-include
    // it — exactly what `ignore::Gitignore` implements and a hand-rolled matcher would not, without real
    // care. (A bare `sub2/` directory-match pattern, by contrast, prunes the whole directory before any
    // child pattern — including a negation — is ever consulted; that is a distinct, well-known gitignore
    // gotcha this test deliberately does NOT exercise, since `sub2/*` is the shape that is actually
    // negatable.)
    let dir = TempDir::new("zzop-gi-negate");
    dir.write(".gitignore", "sub2/*\n!sub2/keep.ts\n");
    dir.write("sub2/drop.ts", "export function drop_() { return 1; }\n");
    dir.write("sub2/keep.ts", "export function keep() { return 2; }\n");

    let out = scan(&dir);
    let walked = walked_files(&out);
    let symbols = symbol_files(&out);

    assert!(
        walked.contains("sub2/keep.ts"),
        "negated pattern must still be analyzed: {walked:?}"
    );
    assert!(
        !walked.contains("sub2/drop.ts"),
        "non-negated sibling stays ignored: {walked:?}"
    );
    assert!(symbols.contains("sub2/keep.ts"), "{symbols:?}");
    assert!(!symbols.contains("sub2/drop.ts"), "{symbols:?}");
}

// --- 4. DEFAULT_SKIP_DIRS still enforced even with no .gitignore present ---

#[test]
fn default_skip_dirs_are_still_skipped_without_any_gitignore() {
    let dir = TempDir::new("zzop-gi-skipdirs");
    dir.write(
        "node_modules/pkg/index.ts",
        "export function pkg() { return 1; }\n",
    );
    dir.write("src/app.ts", "export function app() { return 1; }\n");

    let out = scan(&dir);
    let walked = walked_files(&out);

    assert!(walked.contains("src/app.ts"), "{walked:?}");
    assert!(
        !walked.contains("node_modules/pkg/index.ts"),
        "node_modules stays skipped by DEFAULT_SKIP_DIRS regardless of .gitignore: {walked:?}"
    );
    assert_eq!(out.file_count, 1, "{walked:?}");
}

// --- 4b. self-scan pollution: zzop's OWN default report/cache output dirs must not be rescanned ---
//
// The JS CLI writes reports into `<repo>/zzop-reports/` and defaults `cacheDir` to `.zzop-cache` in its
// `zzop init` template (both, by default, land INSIDE the analyzed tree) — without a default exclusion the
// NEXT analysis walks a prior run's own output as source, growing the file count every run (observed live
// in a blind field test). `DEFAULT_SKIP_DIRS` (`dispatch.rs`) must exclude both by name.

#[test]
fn zzops_own_report_and_cache_dirs_are_not_rescanned_as_source() {
    let dir = TempDir::new("zzop-gi-selfscan");
    dir.write(
        "zzop-reports/zzop.1700000000/be.md",
        "# be\n\nsome prior report content\n",
    );
    dir.write(".zzop-cache/deadbeef.json", "{}");
    dir.write("src/app.ts", "export function app() { return 1; }\n");

    let out = scan(&dir);
    let walked = walked_files(&out);

    assert!(walked.contains("src/app.ts"), "{walked:?}");
    assert!(
        !walked.iter().any(|f| f.starts_with("zzop-reports/")),
        "a prior run's own reports must not be rescanned as source: {walked:?}"
    );
    assert!(
        !walked.iter().any(|f| f.starts_with(".zzop-cache/")),
        "the default cache dir must not be rescanned as source: {walked:?}"
    );
    assert_eq!(out.file_count, 1, "{walked:?}");
}

// --- 5. ancestor .gitignore: root BELOW the git toplevel still honors the toplevel's .gitignore ---
//
// A monorepo where the analysis root (e.g. `repo/apps`) is a subdirectory of the git toplevel
// (`repo/`), and `repo/.gitignore` ignores a build-output directory (`out/`) that reappears, nested,
// under the analysis root (`repo/apps/out/`) — see `pipeline.rs::walk_files` and
// `ancestor_gitignores`'s docs.

#[test]
fn ancestor_gitignore_at_git_toplevel_applies_to_a_subdirectory_analysis_root() {
    let dir = TempDir::new("zzop-gi-ancestor-toplevel");
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    dir.write(".gitignore", "out/\n");
    dir.write("apps/keep.ts", "export function keep() { return 1; }\n");
    dir.write(
        "apps/out/generated.ts",
        "export function generated() { return 2; }\n",
    );
    dir.write(
        "apps/sibling/kept.ts",
        "export function kept() { return 3; }\n",
    );

    let out = scan_root(&dir, "apps");
    let walked = walked_files(&out);
    let symbols = symbol_files(&out);

    assert!(walked.contains("keep.ts"), "{walked:?}");
    assert!(walked.contains("sibling/kept.ts"), "{walked:?}");
    assert!(
        !walked.contains("out/generated.ts"),
        "out/generated.ts is gitignored by the ANCESTOR (git-toplevel) .gitignore, above the analysis \
         root -> must never be walked: {walked:?}"
    );
    assert!(!symbols.contains("out/generated.ts"), "{symbols:?}");
    assert!(symbols.contains("keep.ts"), "{symbols:?}");
    assert!(symbols.contains("sibling/kept.ts"), "{symbols:?}");
    assert_eq!(out.file_count, 2, "{walked:?}");
}

// --- 6. non-git tree: a stray PARENT .gitignore above the analysis root has NO effect (unchanged,
//        deterministic behavior — there is no `.git` to bound the ancestor climb by, so this function does
//        not climb at all, exactly as before this fix) ---

#[test]
fn stray_parent_gitignore_in_a_non_git_tree_is_never_consulted() {
    let dir = TempDir::new("zzop-gi-nongit-stray-parent");
    // Deliberately NO `.git` anywhere in this fixture.
    dir.write(".gitignore", "out/\n");
    dir.write("apps/keep.ts", "export function keep() { return 1; }\n");
    dir.write(
        "apps/out/generated.ts",
        "export function generated() { return 2; }\n",
    );

    let out = scan_root(&dir, "apps");
    let walked = walked_files(&out);

    assert!(walked.contains("keep.ts"), "{walked:?}");
    assert!(
        walked.contains("out/generated.ts"),
        "no .git toplevel exists anywhere above the analysis root, so the stray parent .gitignore must \
         NOT be consulted (unchanged, pre-fix behavior): {walked:?}"
    );
    assert_eq!(out.file_count, 2, "{walked:?}");
}

// --- 7. nested ancestor chain: BOTH the toplevel's .gitignore AND an intermediate directory's .gitignore
//        (still above the analysis root) apply simultaneously ---

#[test]
fn nested_ancestor_gitignore_chain_applies_every_level() {
    let dir = TempDir::new("zzop-gi-ancestor-chain");
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    dir.write(".gitignore", "top_out/\n");
    dir.write("mid/.gitignore", "mid_out/\n");
    dir.write(
        "mid/apps/top_out/x.ts",
        "export function x() { return 1; }\n",
    );
    dir.write(
        "mid/apps/mid_out/y.ts",
        "export function y() { return 2; }\n",
    );
    dir.write("mid/apps/keep.ts", "export function keep() { return 3; }\n");

    let out = scan_root(&dir, "mid/apps");
    let walked = walked_files(&out);

    assert!(walked.contains("keep.ts"), "{walked:?}");
    assert!(
        !walked.contains("top_out/x.ts"),
        "top_out/ is gitignored by the git-TOPLEVEL .gitignore: {walked:?}"
    );
    assert!(
        !walked.contains("mid_out/y.ts"),
        "mid_out/ is gitignored by the INTERMEDIATE ancestor .gitignore (mid/.gitignore, still above the \
         analysis root mid/apps): {walked:?}"
    );
    assert_eq!(out.file_count, 1, "{walked:?}");
}

// --- 8. negation inside an ancestor .gitignore still works, same as it does for a root/nested one ---

#[test]
fn negated_pattern_in_an_ancestor_gitignore_is_still_analyzed() {
    let dir = TempDir::new("zzop-gi-ancestor-negate");
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    dir.write(".gitignore", "*.log\n!important.log\n");
    dir.write("apps/debug.log", "not real ts, just a gitignore probe\n");
    dir.write(
        "apps/important.log",
        "not real ts, just a gitignore probe\n",
    );
    dir.write("apps/keep.ts", "export function keep() { return 1; }\n");

    let out = scan_root(&dir, "apps");
    let walked = walked_files(&out);

    assert!(walked.contains("keep.ts"), "{walked:?}");
    assert!(
        walked.contains("important.log"),
        "negated pattern in the ANCESTOR .gitignore must still be walked: {walked:?}"
    );
    assert!(
        !walked.contains("debug.log"),
        "non-negated sibling stays ignored: {walked:?}"
    );
    assert_eq!(out.file_count, 2, "{walked:?}");
}
