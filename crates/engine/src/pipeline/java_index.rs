//! Java `(package, type)` -> file index: scans every walked `.java` file's own text for its `package`
//! declaration (`zzop_parser_java_21::java_package_of`) and top-level type names
//! (`zzop_parser_java_21::java_type_names`), building the substrate both directions of Java import
//! resolution need — `analyze::assemble::dep_graph::merge_java_dep_edges` (dep-graph edges) AND
//! `analyze::assemble::collect::census`'s F5 drain (unresolved-import census staging) — same dual-call
//! shape `RustWorkspaceMap`/`GoModuleMap` already serve their own two call sites.
//!
//! ## Why a separate engine-side scan, not a `FileArtifact` field
//! The fused per-file pass (`pipeline::compute_fresh_artifact`) has each file's text in hand at parse
//! time, but `FileArtifact` carries no package field — folding one in would mean EVERY other language's
//! artifact construction site also grows a field it never uses (Prisma/TS/Python/Rust/Go all leave it
//! `None`), for a fact only Java's own cross-file resolution needs. Mirrors `go_module.rs`'s manifest-read
//! precedent (`std::fs::read_to_string(root.join(rel))`, degrade to "contributes nothing" on any I/O
//! failure) and `run_java_provides_project_pass`'s own re-read precedent (native_rules.rs — the whole-
//! corpus Java pass already re-reads every `.java` file fresh off disk for the identical reason: a
//! per-file cache cannot see whole-corpus facts).
//!
//! ## Resolution semantics a caller builds on top of this index
//! A plain/static import specifier (`a.b.C` or `a.b.C.m`) needs `(package, type)` split at the RIGHTMOST
//! dot to try first, then — for a static member import, whose specifier carries one segment more than a
//! type import — retried with the last segment trimmed off (`a.b.C.m` -> `a.b.C`) before splitting again.
//! A glob import (`a.b.*`) fans out to EVERY file whose package equals the pre-glob prefix — the Go
//! package-fanout precedent (`merge_go_dep_edges`'s doc): Java has no per-symbol import resolution this
//! index can see either (only whole compilation-unit files), so "imports the package" means "depends on
//! every type declared in it", same honesty argument Go's own package-directory fanout makes. See
//! `analyze::assemble::helpers::resolve_java_import` for the actual resolution function built on this
//! index.

use std::collections::BTreeMap;
use std::path::Path;

/// `(package, simple type name)` -> the one file declaring that top-level type, plus `package` -> every
/// file declaring THAT package (sorted, deterministic via `BTreeMap`/`Vec` built in sorted rel order) —
/// see module doc for why both directions are needed. A file with no `package` declaration (Java's
/// default package) contributes to neither map: a default-package type is never importable from another
/// file at all (JLS), so indexing it would only ever produce unreachable dead entries.
#[derive(Debug, Clone, Default)]
pub(crate) struct JavaIndex {
    pub(crate) by_type: BTreeMap<(String, String), String>,
    pub(crate) by_package: BTreeMap<String, Vec<String>>,
}

/// Scans every walked `.java` file (already filtered by the caller, mirroring `scan_go_modules`'s own
/// "caller pre-filters the iterator" convention) for its package + top-level type names, building a
/// [`JavaIndex`]. An unreadable file (deleted/permission race since the fused pass's own read, or a
/// parse failure inside `java_package_of`/`java_type_names`) contributes nothing for that file — never
/// panics, same degrade-to-nothing discipline `scan_go_modules`/`scan_rust_workspace` uphold for their
/// own manifest/source reads.
pub(crate) fn scan_java_index<'a>(
    root: &Path,
    walked_java_files: impl Iterator<Item = &'a str>,
) -> JavaIndex {
    let mut index = JavaIndex::default();
    // Sorted input (not just sorted output): `walked_java_files` order is whatever the caller's own
    // collection produced, but iterating in path order keeps `by_package`'s per-package file `Vec`
    // deterministic without a separate sort pass at the end.
    let mut rels: Vec<&str> = walked_java_files.collect();
    rels.sort_unstable();
    for rel in rels {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        let Some(package) = zzop_parser_java_21::java_package_of(&text) else {
            continue; // default package — never importable, module doc.
        };
        for type_name in zzop_parser_java_21::java_type_names(&text) {
            index
                .by_type
                .entry((package.clone(), type_name))
                .or_insert_with(|| rel.to_string());
        }
        index
            .by_package
            .entry(package)
            .or_default()
            .push(rel.to_string());
    }
    index
}

#[cfg(test)]
mod tests;
