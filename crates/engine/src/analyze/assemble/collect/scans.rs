//! The four up-front index scans `collect` runs before its artifact loop — Rust workspace manifest,
//! `go.mod` modules, Java `(package, type)`, C# namespaces. Extracted from `collect.rs` (file-size
//! limit); each is a cheap no-op on a tree with no matching-extension file (empty iterator -> empty
//! map/index, no disk I/O). See each scan fn's own doc (`crate::pipeline::scan_rust_workspace`/
//! `scan_go_modules`/`scan_java_index`/`scan_csharp_index`) for its resolution semantics, and each
//! resolver's (`super::super::helpers::resolve_rust_import`/`resolve_go_import_package_dir`/
//! `resolve_java_import`/`resolve_csharp_import`) for how the built index is consumed downstream.

use crate::analyze::assemble::helpers::{
    is_csharp_source_ext, is_go_source_ext, is_java_source_ext, is_rust_source_ext,
};
use crate::pipeline::{
    scan_csharp_index, scan_go_modules, scan_java_index, scan_rust_workspace, CSharpIndex,
    FileArtifact, GoModuleMap, JavaIndex, RustWorkspaceMap,
};

/// Runs all four scans over `artifacts`, filtered to each language's own source extension. Returned as
/// `(rust_workspace, go_modules, java_index, csharp_index)` — the exact order `collect` destructures.
pub(super) fn scan_indices(
    root: &std::path::Path,
    artifacts: &[FileArtifact],
) -> (RustWorkspaceMap, GoModuleMap, JavaIndex, CSharpIndex) {
    let rust_workspace = scan_rust_workspace(
        root,
        artifacts
            .iter()
            .map(|a| a.rel.as_str())
            .filter(|rel| is_rust_source_ext(rel)),
    );
    let go_modules = scan_go_modules(
        root,
        artifacts
            .iter()
            .map(|a| a.rel.as_str())
            .filter(|rel| is_go_source_ext(rel)),
    );
    let java_index = scan_java_index(
        root,
        artifacts
            .iter()
            .map(|a| a.rel.as_str())
            .filter(|rel| is_java_source_ext(rel)),
    );
    let csharp_index = scan_csharp_index(
        root,
        artifacts
            .iter()
            .map(|a| a.rel.as_str())
            .filter(|rel| is_csharp_source_ext(rel)),
    );
    (rust_workspace, go_modules, java_index, csharp_index)
}
