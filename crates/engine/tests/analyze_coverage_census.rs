//! End-to-end test for the per-tree structural coverage census (`zzop_engine::CoverageCensus`, Stage 1 of
//! the "active coverage/blindness disclosure" feature). Mirrors `analyze_cross_layer_findings.rs`'s
//! scaffolding (real TypeScript files written to disk, parsed for real via `zzop_engine::analyze_trees`) —
//! not hand-built `AnalyzeOutput`s. Exercises the census as a PURE post-aggregate over already-assembled
//! data: a provide-only tree, a consume-only tree, and a tree with no io at all (the active-blindness
//! `join_contribution_zero` fact), plus `import_edges`/`symbols` on a tree with a real import and a
//! real symbol.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_trees, EngineConfig};

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

fn config(source_id: &str) -> EngineConfig {
    EngineConfig {
        source_id: source_id.to_string(),
        ..EngineConfig::default()
    }
}

/// BE tree: provides Hono HTTP routes — the "this tree filled the io channel via provides" shape.
fn be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-cov-be");
    dir.write(
        "routes/apiRoutes.ts",
        "const apiRoutes = new Hono();\n\
         apiRoutes.get(\"/authen/getUserInfo\", api.getUserInfo);\n\
         apiRoutes.put(\"/api/v1/orders\", api.updateOrder);\n",
    );
    dir
}

/// FE tree: real `fetch` calls with statically-resolvable literal paths — the "this tree filled the io
/// channel via keyed consumes" shape.
fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-cov-fe");
    dir.write(
        "src/Ctx.tsx",
        "export function ok() { return fetch(\"/authen/getUserInfo\"); }\n\
         export function orders() { return fetch(\"/api/v1/orders\", { method: \"PUT\" }); }\n",
    );
    dir
}

/// A plain tree with no fetch/no routes — no io channel filled at all, but real files/symbols/imports so
/// `files > 0`. Drives the active-blindness `join_contribution_zero` fact.
fn dark_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-cov-dark");
    dir.write("src/util.ts", "export function helper() { return 1; }\n");
    dir.write(
        "src/main.ts",
        "import { helper } from \"./util\";\n\
         export function run() { return helper(); }\n",
    );
    dir
}

#[test]
fn provide_only_tree_has_nonzero_io_provides_and_is_not_join_contribution_zero() {
    let be = be_tree();
    let trees = vec![(be.path().to_path_buf(), config("be"))];
    let out = analyze_trees(&trees);

    let coverage = &out.trees[0].2.coverage;
    assert!(coverage.io_provides > 0, "{coverage:?}");
    assert!(!coverage.join_contribution_zero, "{coverage:?}");
    assert!(coverage.files > 0, "{coverage:?}");
}

#[test]
fn no_io_tree_has_zero_counts_and_is_join_contribution_zero() {
    let dark = dark_tree();
    let trees = vec![(dark.path().to_path_buf(), config("dark"))];
    let out = analyze_trees(&trees);

    let coverage = &out.trees[0].2.coverage;
    assert_eq!(coverage.io_provides, 0, "{coverage:?}");
    assert_eq!(coverage.io_consumes_keyed, 0, "{coverage:?}");
    assert_eq!(coverage.io_consumes_unresolved, 0, "{coverage:?}");
    assert!(coverage.files > 0, "{coverage:?}");
    assert!(coverage.join_contribution_zero, "{coverage:?}");
}

#[test]
fn consume_only_tree_has_keyed_consumes_and_is_not_join_contribution_zero() {
    let fe = fe_tree();
    let be = be_tree();
    // Pair with a BE tree too, so the fetch consumes actually resolve real join edges — the census
    // itself only reads this tree's own assembled io, but the fixture stays realistic end to end.
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    let fe_coverage = &out.trees[0].2.coverage;
    assert!(fe_coverage.io_consumes_keyed > 0, "{fe_coverage:?}");
    assert!(!fe_coverage.join_contribution_zero, "{fe_coverage:?}");
}

/// A tree whose only io fact is an UNRESOLVED consume (dynamic URL argument, no literal to key on) — 0
/// provides, 0 KEYED consumes, but 1 unresolved consume.
fn unresolved_only_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-cov-unresolved");
    dir.write(
        "src/Api.tsx",
        "export function call(x: string) { return axios.get(buildUrl(x)); }\n",
    );
    dir
}

/// Pins the 2026-07-17 redefinition of `join_contribution_zero`: an unresolved consume proves the
/// extractor SAW a call site, but it can never join anything either way (no key to match a provide
/// against), so a tree with 0 provides, 0 keyed consumes, and 1+ unresolved consumes must still count as
/// "no JOINABLE contribution" — before this redefinition the flag also required
/// `io_consumes_unresolved == 0`, which under-fired (stayed `false`) on exactly this tree shape.
#[test]
fn unresolved_only_tree_is_still_join_contribution_zero() {
    let dir = unresolved_only_tree();
    let trees = vec![(dir.path().to_path_buf(), config("unresolved-only"))];
    let out = analyze_trees(&trees);

    let coverage = &out.trees[0].2.coverage;
    assert_eq!(coverage.io_provides, 0, "{coverage:?}");
    assert_eq!(coverage.io_consumes_keyed, 0, "{coverage:?}");
    assert!(coverage.io_consumes_unresolved > 0, "{coverage:?}");
    assert!(
        coverage.join_contribution_zero,
        "an unresolved-only consume can never join anything either way — must still count as zero \
joinable contribution: {coverage:?}"
    );
}

#[test]
fn import_edges_and_symbols_are_nonzero_for_a_tree_with_an_import_and_a_symbol() {
    let dark = dark_tree();
    let trees = vec![(dark.path().to_path_buf(), config("dark"))];
    let out = analyze_trees(&trees);

    let coverage = &out.trees[0].2.coverage;
    assert!(coverage.import_edges > 0, "{coverage:?}");
    assert!(coverage.symbols > 0, "{coverage:?}");
}

#[test]
fn files_field_matches_file_count() {
    let be = be_tree();
    let trees = vec![(be.path().to_path_buf(), config("be"))];
    let out = analyze_trees(&trees);

    let (_, _, output) = &out.trees[0];
    assert_eq!(output.coverage.files, output.file_count);
}
