//! C# namespace -> files dep-graph index: scans every walked `.cs` file's own text for every namespace
//! it declares (`zzop_parser_csharp::csharp_namespaces_of`), building the substrate C# `using` resolution
//! needs — `analyze::assemble::dep_graph::merge_csharp_dep_edges` (dep-graph edges) AND
//! `analyze::assemble::collect::census`'s C# drain (unresolved-import census staging) — same dual-call
//! shape `JavaIndex`/`GoModuleMap`/`RustWorkspaceMap` already serve their own two call sites.
//!
//! ## Simpler than Java, by construction
//! A C# `using Foo.Bar;` directive is always NAMESPACE-level (never a single-type import the way Java's
//! `import a.b.C;` is), so this index needs only ONE map — no `by_type` twin the way `JavaIndex` needs
//! both directions. Resolution (`analyze::assemble::helpers::resolve_csharp_import`) treats an import
//! specifier as a namespace name and looks it up here directly; there is no rightmost-dot-split retry
//! Java's own resolver needs, since a C# specifier is already namespace-shaped, never
//! `namespace.TypeName`. A `using static Foo.Bar;` / `using Alias = Foo.Bar;` whose specifier names a
//! TYPE rather than a namespace simply won't match any key here and resolves to nothing — an
//! under-approximation accepted for the same honesty reason `JavaIndex`'s own package-fanout doc gives:
//! this index has no by-type direction to try instead.
//!
//! ## Why a separate engine-side scan, not a `FileArtifact` field
//! Mirrors `java_index.rs`'s own doc verbatim: the fused per-file pass has each file's text in hand at
//! parse time, but folding a namespace-list field into `FileArtifact` would mean every other language's
//! artifact construction site also grows a field it never uses. A separate `std::fs::read_to_string`
//! re-read (degrading to "contributes nothing" on any I/O failure) mirrors `go_module.rs`'s /
//! `scan_java_index`'s identical precedent.

use std::collections::BTreeMap;
use std::path::Path;

/// Namespace -> every file declaring that namespace (sorted, deterministic via `BTreeMap`/`Vec` built in
/// sorted rel order) — see module doc for why one map suffices (unlike `JavaIndex`'s two).
#[derive(Debug, Clone, Default)]
pub(crate) struct CSharpIndex {
    pub(crate) by_namespace: BTreeMap<String, Vec<String>>,
}

/// Scans every walked `.cs` file (already filtered by the caller, mirroring `scan_java_index`'s own
/// "caller pre-filters the iterator" convention) for every namespace it declares, building a
/// [`CSharpIndex`]. An unreadable file (deleted/permission race since the fused pass's own read, or a
/// parse failure inside `csharp_namespaces_of`) contributes nothing for that file — never panics, same
/// degrade-to-nothing discipline `scan_java_index`/`scan_go_modules`/`scan_rust_workspace` uphold for
/// their own manifest/source reads.
pub(crate) fn scan_csharp_index<'a>(
    root: &Path,
    walked_csharp_files: impl Iterator<Item = &'a str>,
) -> CSharpIndex {
    let mut index = CSharpIndex::default();
    // Sorted input (not just sorted output): `walked_csharp_files` order is whatever the caller's own
    // collection produced, but iterating in path order keeps `by_namespace`'s per-namespace file `Vec`
    // deterministic without a separate sort pass at the end — mirrors `scan_java_index`'s identical
    // reasoning for `by_package`.
    let mut rels: Vec<&str> = walked_csharp_files.collect();
    rels.sort_unstable();
    for rel in rels {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        for namespace in zzop_parser_csharp::csharp_namespaces_of(&text) {
            index
                .by_namespace
                .entry(namespace)
                .or_default()
                .push(rel.to_string());
        }
    }
    index
}

#[cfg(test)]
mod tests;
