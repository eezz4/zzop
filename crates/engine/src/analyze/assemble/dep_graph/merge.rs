//! The Python/Rust/Go/Java dep-graph edge-merge trio (quartet) — split out from `dep_graph.rs` purely to
//! keep that file under the line-count ratchet; `build()` (the sole call site for all four) stays in the
//! parent module. See each function's own doc for the resolution semantics; `build()`'s own call-site
//! comments explain why the four are deliberately NOT generalized into one shared implementation.

use std::collections::{BTreeMap, HashSet};

use zzop_core::{ir::DepGraph, ImportMap};

use crate::pipeline::{GoModuleMap, JavaIndex, RustWorkspaceMap};

use super::super::helpers::{
    is_go_source_ext, is_go_test_file, is_java_source_ext, is_python_source_ext,
    is_rust_source_ext, resolve_go_import_package_dir, resolve_java_import, resolve_python_import,
    resolve_rust_import,
};

/// Merges Python import edges into `dep`, in place — the dep-graph half of the resolver wiring
/// [`super::super::helpers::resolve_python_import`]'s doc describes. Every Python file already has a
/// `dep` entry (possibly empty) from the `build_dep_with_workspace` call just above this function's one
/// call site: a Python relative specifier (`./sib`) never matches `zzop_parser_typescript::resolve`'s
/// `try_ext` (which only tries `.ts`/`.tsx`/`.js`/`.jsx`/`.mjs`/`.cjs`/index-file suffixes, never
/// `.py`/`__init__.py`), and a Python absolute dotted specifier (`a.b.c`, or a bare external name like
/// `fastapi`) never matches a governing tsconfig's `paths`/`baseUrl` or a registered workspace package
/// by sheer construction (no `/` for `match_workspace_pkg` to split on, and an astronomically unlikely
/// tsconfig `paths` collision) — so the TS resolver's own pass over these SAME bindings (they ride the
/// shared `ts_import_pairs`, `ts_slot`'s participation) contributes nothing for a Python file, safe to
/// add to afterward without a remove/overwrite step. See `pipeline.rs`'s "SAFETY SWEEP" note for the
/// exhaustive check this reasoning is pinned against.
///
/// `binding.original == "*"` (a plain `import a.b.c`, or a star import) is translated to `None` before
/// calling `resolve_python_import` — `"*"` never names a real submodule, matching
/// `zzop_parser_python_3::python_import_candidates`'s own documented contract for when to omit `original`.
pub(super) fn merge_python_dep_edges(
    dep: &mut DepGraph,
    ts_import_pairs: &[(String, ImportMap)],
    all_paths: &HashSet<String>,
) {
    for (rel, imports) in ts_import_pairs {
        if !is_python_source_ext(rel) {
            continue;
        }
        let entry = dep.entry(rel.clone()).or_default();
        let mut seen: HashSet<String> = entry.iter().cloned().collect();
        for binding in imports.values() {
            let original = (binding.original != "*").then_some(binding.original.as_str());
            if let Some(target) =
                resolve_python_import(&binding.specifier, original, rel, all_paths)
            {
                if seen.insert(target.clone()) {
                    entry.push(target);
                }
            }
        }
    }
}

/// Merges Rust import edges into `dep`, in place — the additive dep-graph twin of
/// `merge_python_dep_edges` above, for `zzop_parser_rust::rust_import_candidates`-shaped specifiers
/// instead. Every Rust file already has a `dep` entry (possibly empty) from the `build_dep_with_workspace`
/// call above (`ts_import_pairs` carries its `ImportMap` too, via `ts_slot`'s shared participation — see
/// `pipeline::FileArtifact::imports`'s doc), and a `.rs` specifier never matches the TS resolver's own
/// `try_ext`/tsconfig-`paths`/workspace-package machinery by sheer construction (no `.ts`/`.js` suffix, no
/// `/`-headed workspace-alias shape), so this only adds edges, never removes what's there — same safety
/// argument `merge_python_dep_edges`'s own doc makes.
///
/// Unlike `merge_python_dep_edges`, EVERY binding is tried (not gated to non-`crate`/`super`/`self` heads
/// the way `collect::collect`'s package-census staging is) — `resolve_rust_import` itself already returns
/// `None` for a `crate`/`super`/`self` head that resolves to nothing, and for any external head with no
/// workspace-member match, so no extra pre-filter is needed here.
pub(super) fn merge_rust_dep_edges(
    dep: &mut DepGraph,
    ts_import_pairs: &[(String, ImportMap)],
    all_paths: &HashSet<String>,
    rust_workspace: &RustWorkspaceMap,
) {
    for (rel, imports) in ts_import_pairs {
        if !is_rust_source_ext(rel) {
            continue;
        }
        let entry = dep.entry(rel.clone()).or_default();
        let mut seen: HashSet<String> = entry.iter().cloned().collect();
        for binding in imports.values() {
            if let Some(target) =
                resolve_rust_import(&binding.specifier, rel, all_paths, rust_workspace)
            {
                if seen.insert(target.clone()) {
                    entry.push(target);
                }
            }
        }
    }
}

/// Merges Go import edges into `dep`, in place — the additive dep-graph twin of
/// `merge_python_dep_edges`/`merge_rust_dep_edges` above, for `resolve_go_import_package_dir`-resolved
/// specifiers.
///
/// **Semantic decision: a Go import targets a PACKAGE, not a file.** Go's own compilation unit is the
/// package (a directory), not the individual `.go` file — every non-test file directly in that directory
/// is compiled together, sharing symbols with no import statement between them at all. So `import
/// "example.com/app/internal/db"` does not import "one file that happens to define what I need"; it
/// imports the WHOLE `internal/db` package. This function honors that literally: for each resolved
/// import, an edge is emitted from the importing file to EVERY walked, non-`_test.go` `.go` file
/// DIRECTLY in the resolved package directory (non-recursive — a subdirectory is a DIFFERENT package in
/// Go, never pulled in transitively by importing the parent). Two consequences, both intended: (1) this
/// is semantically honest — the importing file really does depend on the full compiled unit, not just
/// whichever single file we might otherwise have guessed; (2) EVERY file in an imported package gets real
/// `fan_in` from every importer, not just the file that happens to declare the specific symbol used — a
/// package file with zero direct importers of ITS OWN symbols but siblings that are imported would
/// otherwise false-positive `dead-candidates`. `_test.go` files are excluded from the TARGET side only
/// (task 5): a test file is never part of the importable package from another package's perspective (Go
/// excludes `_test.go` files from a package's own compiled archive), though a `_test.go` file can still be
/// a SOURCE of its own outgoing edges (its own `import` statements are ordinary Go imports).
///
/// Every Go file already has a `dep` entry (possibly empty) from the `build_dep_with_workspace` call
/// above (`ts_import_pairs` carries its `ImportMap` too, via `ts_slot`'s shared participation — see
/// `pipeline::FileArtifact::imports`'s doc), and a Go import-path specifier never matches the TS
/// resolver's own `try_ext`/tsconfig-`paths`/workspace-package machinery by sheer construction (no
/// `.ts`/`.js` suffix, no relative-path shape), so this only adds edges, never removes what's there —
/// same safety argument `merge_python_dep_edges`'s own doc makes.
pub(super) fn merge_go_dep_edges(
    dep: &mut DepGraph,
    ts_import_pairs: &[(String, ImportMap)],
    all_paths: &HashSet<String>,
    go_modules: &GoModuleMap,
) {
    // Package directory -> its own non-test `.go` files (sorted, deterministic) — computed once, not per
    // import, since every resolved import needs the same lookup.
    let mut dir_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in all_paths {
        if !is_go_source_ext(path) || is_go_test_file(path) {
            continue;
        }
        let dir = match path.rfind('/') {
            Some(idx) => path[..idx].to_string(),
            None => String::new(),
        };
        dir_files.entry(dir).or_default().push(path.clone());
    }
    for files in dir_files.values_mut() {
        files.sort();
    }

    for (rel, imports) in ts_import_pairs {
        if !is_go_source_ext(rel) {
            continue;
        }
        let entry = dep.entry(rel.clone()).or_default();
        let mut seen: HashSet<String> = entry.iter().cloned().collect();
        for binding in imports.values() {
            let Some(target_dir) =
                resolve_go_import_package_dir(&binding.specifier, rel, go_modules)
            else {
                continue;
            };
            let Some(files) = dir_files.get(&target_dir) else {
                continue;
            };
            for file in files {
                if seen.insert(file.clone()) {
                    entry.push(file.clone());
                }
            }
        }
    }
}

/// Merges Java import edges into `dep`, in place — the additive dep-graph twin of
/// `merge_python_dep_edges`/`merge_rust_dep_edges`/`merge_go_dep_edges` above, for
/// `resolve_java_import`-resolved specifiers (`helpers::resolve_java_import`'s doc has the full
/// resolution-order pin: plain/static-member imports resolve to at most one file, a glob import fans out
/// to every file in the target package — same package-directory-wide fanout reasoning
/// `merge_go_dep_edges`'s own doc gives for Go, since Java's compilation unit for glob-import purposes is
/// likewise "the whole package", not one file).
///
/// Every Java file already has a `dep` entry (possibly empty) from the `build_dep_with_workspace` call
/// above (`ts_import_pairs` carries its `ImportMap` too, via `ts_slot`'s shared participation — see
/// `pipeline::FileArtifact::imports`'s doc), and a Java dotted specifier never matches the TS resolver's
/// own `try_ext`/tsconfig-`paths`/workspace-package machinery by sheer construction (no `.ts`/`.js`
/// suffix, no relative-path shape), so this only adds edges, never removes what's there — same safety
/// argument `merge_python_dep_edges`'s own doc makes.
pub(super) fn merge_java_dep_edges(
    dep: &mut DepGraph,
    ts_import_pairs: &[(String, ImportMap)],
    java_index: &JavaIndex,
) {
    for (rel, imports) in ts_import_pairs {
        if !is_java_source_ext(rel) {
            continue;
        }
        let entry = dep.entry(rel.clone()).or_default();
        let mut seen: HashSet<String> = entry.iter().cloned().collect();
        for binding in imports.values() {
            for target in resolve_java_import(&binding.specifier, java_index) {
                if seen.insert(target.clone()) {
                    entry.push(target);
                }
            }
        }
    }
}
