//! Pin for the cross-tree package-import disclosure: when a tree's own package-import census names
//! ANOTHER tree analyzed in the same run, `analyze_trees` must say so on that tree's own
//! `AnalyzeOutput::warnings`.
//!
//! The measured defect (22-package pnpm monorepo, `{"trees": "auto"}`): 5,262 dep-graph links, of which
//! ZERO crossed a package boundary; the identical tree analyzed as ONE tree produced 6,651 links of which
//! 1,364 crossed one, over 23 distinct package pairs. The resolver was never broken — `trees: "auto"`
//! gives each workspace package its own tree, a dep graph is built per tree, and an import crossing a
//! tree boundary is therefore censused as an EXTERNAL package instead of becoming a dep edge. The reader
//! saw "22 independent islands" and nothing in the output contradicted that.
//!
//! The judgment pinned here is an OBSERVATION, never a guess: the specifiers reported are exactly those
//! in this tree's `package_imports` that match another tree's `source_id` in this same run — a set
//! intersection over data zzop already holds. Matching reuses
//! `zzop_parser_typescript::match_workspace_pkg` (exact, scoped sub-path, unscoped sub-path), never a
//! second matcher.
//!
//! Coverage:
//! - The importing tree's own warnings carry exactly one such disclosure, naming the matched specifiers
//!   with their `file_count` and `example_file`.
//! - The disclosure states the mechanism, the blast radius, AND the remedy WITH its cost (the one-tree
//!   remedy turns the cross-layer join off) — the "a remedy must name its cost" invariant.
//! - A genuinely external package (`react`, `zod`) is never reported.
//! - A specifier matching the tree's OWN source id is never reported.
//! - The imported tree, which imports nothing, gets no warning.
//! - A single-tree run gets no warning (there is no other tree to cross into).
//! - More matches than the example cap truncates OUT LOUD, in sorted specifier order.

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

const FE_SOURCE_ID: &str = "@apps/tool-hub-fe";
const UTILS_SOURCE_ID: &str = "@base/utils-fe";

/// The app package: two imports of the sibling workspace package (one bare, one scoped sub-path), two
/// genuinely external packages, and one import of its OWN package name — the four cases the disclosure
/// has to tell apart, in one tree.
fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xtree-fe");
    dir.write("package.json", r#"{"name": "@apps/tool-hub-fe"}"#);
    dir.write(
        "src/api.ts",
        "import { hash } from \"@base/utils-fe/auth/hash\";\n\
         import React from \"react\";\n\
         export const signIn = () => hash(React);\n",
    );
    dir.write(
        "src/app.ts",
        "import { fmt } from \"@base/utils-fe\";\n\
         import { z } from \"zod\";\n\
         export const render = () => fmt(z);\n",
    );
    dir.write(
        "src/self.ts",
        "import { own } from \"@apps/tool-hub-fe/lib/own\";\n\
         export const reuse = () => own();\n",
    );
    dir
}

/// The shared package: it is the TARGET of the cross-tree imports and imports nothing itself.
fn utils_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xtree-utils");
    dir.write("package.json", r#"{"name": "@base/utils-fe"}"#);
    dir.write(
        "src/index.ts",
        "export const fmt = (x: unknown) => String(x);\n",
    );
    dir.write(
        "auth/hash.ts",
        "export const hash = (x: unknown) => String(x);\n",
    );
    dir
}

/// PREFIX, not `contains`. A sibling warning may legitimately CITE this one by name — the zero-edge
/// diagnostic does exactly that since 2026-08-08, because on a split workspace the tree boundary is the
/// real cause of its zero and it now points here instead of offering a closed two-way choice. Under a
/// substring filter that citation counted as a second disclosure and every `assert_eq!(…, 1)` below
/// went red. A message's identity is where it starts.
fn disclosures(warnings: &[String]) -> Vec<&String> {
    warnings
        .iter()
        .filter(|w| w.starts_with("cross-tree package imports:"))
        .collect()
}

fn warnings_for<'a>(out: &'a zzop_engine::MultiAnalyzeOutput, source_id: &str) -> &'a Vec<String> {
    &out.trees
        .iter()
        .find(|(_, source, _)| source == source_id)
        .unwrap_or_else(|| panic!("tree {source_id} present"))
        .2
        .warnings
}

#[test]
fn a_tree_importing_another_trees_package_name_is_told_those_imports_are_not_dep_edges() {
    let fe = fe_tree();
    let utils = utils_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config(FE_SOURCE_ID)),
        (utils.path().to_path_buf(), config(UTILS_SOURCE_ID)),
    ];
    let out = analyze_trees(&trees);

    let fe_warnings = warnings_for(&out, FE_SOURCE_ID);
    let found = disclosures(fe_warnings);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one cross-tree import disclosure, got: {fe_warnings:?}"
    );
    let w = found[0];

    // (1) the OBSERVATION — how many specifiers, each with its file_count and example_file.
    assert!(w.contains("2 import specifier(s)"), "{w}");
    assert!(
        w.contains("@base/utils-fe (1 file(s), e.g. src/app.ts)"),
        "{w}"
    );
    assert!(
        w.contains("@base/utils-fe/auth/hash (1 file(s), e.g. src/api.ts)"),
        "{w}"
    );
    // (2) the MECHANISM.
    assert!(
        w.contains("dependency graph holds in-tree edges only"),
        "{w}"
    );
    assert!(w.contains("censused as an EXTERNAL package"), "{w}");
    // (3) the BLAST RADIUS.
    assert!(w.contains("import cycles"), "{w}");
    assert!(w.contains("fan-in/fan-out"), "{w}");
    assert!(w.contains("dead/unimported exports"), "{w}");
    assert!(w.contains("`dep` graph domain"), "{w}");
    // (4) the REMEDY and its COST.
    assert!(w.contains("\"roots\": [\".\"]"), "{w}");
    assert!(w.contains("at the cost of the cross-layer join"), "{w}");

    // Genuinely external packages are not other trees — never reported.
    assert!(!w.contains("react"), "{w}");
    assert!(!w.contains("zod"), "{w}");
    // Nor is the tree's own source id: a specifier naming yourself is not a cross-tree reference.
    assert!(!w.contains(FE_SOURCE_ID), "{w}");

    // The imported tree imports nothing, so it has nothing to disclose.
    assert!(
        disclosures(warnings_for(&out, UTILS_SOURCE_ID)).is_empty(),
        "{:?}",
        warnings_for(&out, UTILS_SOURCE_ID)
    );
}

#[test]
fn a_single_tree_run_has_no_other_tree_to_cross_into_and_stays_silent() {
    // The disclosure is a set intersection with the OTHER trees of this run — with none, it is empty,
    // and `@base/utils-fe` is then an ordinary unresolved package import with nothing to say about it.
    let fe = fe_tree();
    let trees = vec![(fe.path().to_path_buf(), config(FE_SOURCE_ID))];
    let out = analyze_trees(&trees);
    let fe_warnings = warnings_for(&out, FE_SOURCE_ID);
    assert!(disclosures(fe_warnings).is_empty(), "got: {fe_warnings:?}");
}

#[test]
fn more_matches_than_the_example_cap_are_truncated_out_loud_in_sorted_order() {
    let fe = TempDir::new("zzop-engine-xtree-many");
    fe.write("package.json", r#"{"name": "@apps/tool-hub-fe"}"#);
    fe.write(
        "src/wide.ts",
        "import { a } from \"@base/utils-fe\";\n\
         import { b } from \"@base/utils-fe/aa\";\n\
         import { c } from \"@base/utils-fe/bb\";\n\
         import { d } from \"@base/utils-fe/cc\";\n\
         import { e } from \"@base/utils-fe/dd\";\n\
         export const all = [a, b, c, d, e];\n",
    );
    let utils = utils_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config(FE_SOURCE_ID)),
        (utils.path().to_path_buf(), config(UTILS_SOURCE_ID)),
    ];
    let out = analyze_trees(&trees);

    let fe_warnings = warnings_for(&out, FE_SOURCE_ID);
    let found = disclosures(fe_warnings);
    assert_eq!(found.len(), 1, "got: {fe_warnings:?}");
    let w = found[0];

    assert!(w.contains("5 import specifier(s)"), "{w}");
    // Truncation is stated, never silent — and the kept three are the first three in sorted specifier
    // order, which is the ordering `package_imports` (a `BTreeMap` fold) already arrives in.
    assert!(
        w.contains("showing the first 3 of 5, sorted by specifier"),
        "{w}"
    );
    assert!(w.contains("@base/utils-fe (1 file(s)"), "{w}");
    assert!(w.contains("@base/utils-fe/aa (1 file(s)"), "{w}");
    assert!(w.contains("@base/utils-fe/bb (1 file(s)"), "{w}");
    assert!(!w.contains("@base/utils-fe/cc"), "{w}");
    assert!(!w.contains("@base/utils-fe/dd"), "{w}");
}
