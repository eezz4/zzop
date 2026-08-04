//! End-to-end test for the per-tree structural coverage census (`zzop_engine::CoverageCensus`, Stage 1 of
//! the "active coverage/blindness disclosure" feature). Mirrors `analyze_cross_layer_findings.rs`'s
//! scaffolding (real TypeScript files written to disk, parsed for real via `zzop_engine::analyze_trees`) —
//! not hand-built `AnalyzeOutput`s. Exercises the census as a PURE post-aggregate over already-assembled
//! data: a provide-only tree, a consume-only tree, and a tree with no io at all (the active-blindness
//! `join_contribution_zero` fact), plus `resolved_import_edges`/`symbols` on a tree with a real import
//! and a real symbol.

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
fn resolved_import_edges_and_symbols_are_nonzero_for_a_tree_with_an_import_and_a_symbol() {
    let dark = dark_tree();
    let trees = vec![(dark.path().to_path_buf(), config("dark"))];
    let out = analyze_trees(&trees);

    let coverage = &out.trees[0].2.coverage;
    assert!(coverage.resolved_import_edges > 0, "{coverage:?}");
    assert!(coverage.symbols > 0, "{coverage:?}");
}

/// A tree whose imports are ALL package imports — nothing resolves to a walked file. This is the shape
/// the field's old name (`import_edges`) misdescribed, and the reason it was renamed on 2026-07-31.
fn package_imports_only_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-cov-pkg-imports");
    dir.write(
        "src/app.ts",
        "import React from \"react\";\n\
         import axios from \"axios\";\n\
         import { z } from \"zod\";\n\
         export function run() { return React; }\n",
    );
    dir
}

/// Pins the MEMBERSHIP RULE the new name states: `resolved_import_edges` sums out-degrees over the dep
/// graph, and the dep graph holds resolved IN-TREE edges only. A file with three real, syntactically
/// present package imports contributes ZERO here — they are dropped during dep resolution, so they can
/// never be summed. The field is named for that rule now precisely because this zero is not "this file
/// imports nothing": a 91-file Python tree reporting 3 edges was read exactly that way.
#[test]
fn resolved_import_edges_excludes_package_imports_that_resolve_to_no_walked_file() {
    let dir = package_imports_only_tree();
    let trees = vec![(dir.path().to_path_buf(), config("pkg-imports"))];
    let out = analyze_trees(&trees);
    let (_, _, tree) = &out.trees[0];

    assert!(tree.coverage.parser_dispatched > 0, "{:?}", tree.coverage);
    assert!(tree.coverage.symbols > 0, "{:?}", tree.coverage);
    assert_eq!(
        tree.coverage.resolved_import_edges, 0,
        "three package imports are real imports and none of them resolves in-tree — the count is \
         RESOLVED edges, which is what the name now says: {:?}",
        tree.coverage
    );
}

#[test]
fn files_field_matches_file_count() {
    let be = be_tree();
    let trees = vec![(be.path().to_path_buf(), config("be"))];
    let out = analyze_trees(&trees);

    let (_, _, output) = &out.trees[0];
    assert_eq!(output.coverage.files, output.file_count);
}

// --- `parser_dispatched`: the parser-claimed subset of the walked total ---

/// A tree whose walk visits far more files than a parser claims — the shape that made a field run's
/// `fileCount: 4790` read as "this repo has 4,790 code files" when roughly 3,178 carried code.
fn mixed_source_and_asset_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-cov-mixed");
    dir.write("src/app.ts", "export function run() { return 1; }\n");
    dir.write("src/util.ts", "export function helper() { return 2; }\n");
    dir.write("README.md", "# docs\n\nprose, not code.\n");
    dir.write("docs/guide.md", "# guide\n");
    dir.write(
        "assets/logo.png",
        "not-really-a-png-but-walked-all-the-same\n",
    );
    dir.write("data/fixture.json", "{ \"a\": 1 }\n");
    dir
}

#[test]
fn parser_dispatched_counts_only_what_a_parser_claims_while_files_keeps_counting_the_walk() {
    let dir = mixed_source_and_asset_tree();
    let trees = vec![(dir.path().to_path_buf(), config("mixed"))];
    let out = analyze_trees(&trees);
    let (_, _, tree) = &out.trees[0];

    // `files` is unchanged and still means "files walked" — every file under the root, docs/data/assets
    // included. Narrowing it would silently redefine a published output field.
    assert_eq!(tree.coverage.files, 6, "{:?}", tree.coverage);
    assert_eq!(tree.coverage.files, tree.file_count);
    // `parser_dispatched` is the honest repo-size number: the two `.ts` files, not the md/png/json.
    assert_eq!(tree.coverage.parser_dispatched, 2, "{:?}", tree.coverage);
}

#[test]
fn parser_dispatched_equals_files_on_an_all_source_tree() {
    // No breakdown to report when everything walked is code — `files - parser_dispatched == 0`.
    let dir = TempDir::new("zzop-engine-cov-all-source");
    dir.write("a.ts", "export const a = 1;\n");
    dir.write("b.ts", "export const b = 2;\n");
    let trees = vec![(dir.path().to_path_buf(), config("all-source"))];
    let out = analyze_trees(&trees);
    let (_, _, tree) = &out.trees[0];
    assert_eq!(tree.coverage.files, 2);
    assert_eq!(tree.coverage.parser_dispatched, 2);
}

// --- F4: `declared_imports_by_ext` — the declared-side denominator for `resolved_import_edges` ---

/// The F4 pin proper: the same three package imports the resolved-edge test above shows being dropped
/// stay COUNTED on the declared side — `declared 3` next to `resolvedImportEdges 0` is the
/// import-resolution blindness ratio the bare edge number could never show.
#[test]
fn declared_imports_keep_counting_the_package_imports_resolution_drops() {
    let dir = package_imports_only_tree();
    let trees = vec![(dir.path().to_path_buf(), config("pkg-imports-declared"))];
    let out = analyze_trees(&trees);
    let (_, _, tree) = &out.trees[0];

    assert_eq!(
        tree.coverage.resolved_import_edges, 0,
        "{:?}",
        tree.coverage
    );
    assert_eq!(
        tree.coverage.declared_imports_by_ext.get("ts"),
        Some(&3),
        "react/axios/zod are three declared specifiers whether or not any resolves: {:?}",
        tree.coverage.declared_imports_by_ext
    );
}

/// The motivating low-resolution tree shape (`analyze_python_package_roots.rs`'s editable-install
/// fixture, the 91-files/3-edges class): every internal import is absolute under a package name no
/// tree directory carries, so the dep graph stays EMPTY — and before F4 nothing in the output said
/// the tree had declared anything at all. The denominator must surface: declared > 0, resolved 0.
#[test]
fn a_low_resolution_python_tree_surfaces_declared_over_zero_resolved() {
    let dir = TempDir::new("zzop-engine-cov-f4-py");
    dir.write("projects/home/model.py", "def build():\n    return 1\n");
    dir.write(
        "core/train.py",
        "from tml.projects.home import model\n\ndef run():\n    return model.build()\n",
    );
    let trees = vec![(dir.path().to_path_buf(), config("py-low-res"))];
    let out = analyze_trees(&trees);
    let (_, _, tree) = &out.trees[0];

    assert_eq!(
        tree.coverage.resolved_import_edges, 0,
        "{:?}",
        tree.coverage
    );
    let declared_py = tree
        .coverage
        .declared_imports_by_ext
        .get("py")
        .copied()
        .expect("py has an import channel, so the key must be MEASURED even at low resolution");
    assert!(
        declared_py > 0,
        "the declared side must survive the resolution drop: {:?}",
        tree.coverage.declared_imports_by_ext
    );
}

/// Normal-tree parity: on a tree with no glob-fanout imports the declared total covers the resolved
/// total (each edge came from a counted specifier), and a parsed file with ZERO imports still lands
/// in its extension's sum as a measured 0 — `util.ts` contributes nothing, yet `ts` stays 1, not 2.
#[test]
fn declared_imports_cover_resolved_edges_on_a_normal_tree() {
    let dark = dark_tree();
    let trees = vec![(dark.path().to_path_buf(), config("dark-declared"))];
    let out = analyze_trees(&trees);
    let (_, _, tree) = &out.trees[0];

    assert_eq!(
        tree.coverage.resolved_import_edges, 1,
        "{:?}",
        tree.coverage
    );
    assert_eq!(
        tree.coverage.declared_imports_by_ext.get("ts"),
        Some(&1),
        "main.ts declares './util' and util.ts declares nothing (measured 0): {:?}",
        tree.coverage.declared_imports_by_ext
    );
    let declared_total: usize = tree.coverage.declared_imports_by_ext.values().sum();
    assert!(declared_total >= tree.coverage.resolved_import_edges);
}

/// The never-guess half: an extension whose parser projects NO import channel (prisma — and docs/data
/// files, which no parser claims) gets no key at all, so absence stays distinguishable from a
/// measured 0 and the facade can render UNMEASURED instead of a fake zero.
#[test]
fn channel_less_extensions_are_absent_from_declared_imports_not_zero() {
    let dir = TempDir::new("zzop-engine-cov-f4-channelless");
    dir.write(
        "src/app.ts",
        "import React from \"react\";\nexport const a = 1;\n",
    );
    dir.write("db/schema.prisma", "model User {\n  id Int @id\n}\n");
    dir.write("README.md", "# docs\n");
    let trees = vec![(dir.path().to_path_buf(), config("channel-less"))];
    let out = analyze_trees(&trees);
    let (_, _, tree) = &out.trees[0];

    let declared = &tree.coverage.declared_imports_by_ext;
    assert_eq!(declared.get("ts"), Some(&1), "{declared:?}");
    assert!(
        !declared.contains_key("prisma") && !declared.contains_key("md"),
        "no import channel means no key — absence, never 0: {declared:?}"
    );
}

#[test]
fn parser_dispatched_is_zero_when_the_walk_finds_no_parseable_file_at_all() {
    // The disclosure that matters most: a tree that looks non-empty (`files > 0`) but where zzop parsed
    // nothing. Reading `files` alone here is exactly the mis-sizing the breakdown exists to prevent.
    let dir = TempDir::new("zzop-engine-cov-no-source");
    dir.write("README.md", "# docs\n");
    dir.write("data/fixture.json", "{}\n");
    let trees = vec![(dir.path().to_path_buf(), config("no-source"))];
    let out = analyze_trees(&trees);
    let (_, _, tree) = &out.trees[0];
    assert_eq!(tree.coverage.files, 2);
    assert_eq!(tree.coverage.parser_dispatched, 0);
}
