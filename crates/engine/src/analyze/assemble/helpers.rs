//! Small shared helpers used by two or more `assemble` phases: Python, Rust, Go, and Java
//! source-extension / import-resolution glue. Native-analysis timing bookkeeping
//! (`record_native_timing`/`sort_rule_timings`) lives one level up, at `crate::analyze`, since
//! `analyze::native_rules` (a sibling of `assemble`, not a descendant) needs it too.

use std::collections::HashSet;

mod csharp;
mod go;
mod java;

pub(super) use csharp::{is_csharp_source_ext, is_csharp_std_import, resolve_csharp_import};
pub(super) use java::{is_java_source_ext, is_java_std_import, java_census_key};
// Wider than its siblings on purpose: the call-graph pass (`analyze::native_rules::callgraph::
// java_bridge`) resolves the same specifiers to join two Java node-id spaces — see that module's doc.
pub(in crate::analyze) use java::resolve_java_import;

/// Deterministic `(kind, key, file, line)` total order for the tree's final IO provide array — applied
/// right before `IoFacts` assembly in `super::assemble` so emitted order is stable across runs regardless
/// of collection order. Its consume-side twin `sort_io_consumes` uses the identical key order.
/// Extension -> count of files a parser frontend projected at least one fact from, for the
/// zero-extraction cross S17 reads. Three channels, not one: symbols, dep edges AND io facts — a
/// frontend can legitimately project only the third (`zzop-parser-sql`, Prisma), and omitting it
/// understates the denominator and silently narrows every row the cross can produce. Degraded files
/// are excluded, matching the coverage reply's own `structural` column exactly.
pub(super) fn structural_exts(
    rels: &[&str],
    all_symbols: &[zzop_core::ir::SourceSymbol],
    dep: &std::collections::HashMap<String, Vec<String>>,
    io_provides: &[zzop_core::IoProvide],
    io_consumes: &[zzop_core::IoConsume],
    degraded: &[super::collect::DegradedFile],
) -> std::collections::BTreeMap<String, usize> {
    let structural: std::collections::HashSet<&str> = all_symbols
        .iter()
        .map(|s| s.file.as_str())
        .chain(dep.keys().map(String::as_str))
        .chain(io_provides.iter().map(|p| p.file.as_str()))
        .chain(io_consumes.iter().map(|c| c.file.as_str()))
        .collect();
    let degraded: std::collections::HashSet<&str> =
        degraded.iter().map(|d| d.rel.as_str()).collect();
    crate::zero_extraction::structural_by_ext(rels.iter().copied(), &structural, &degraded)
}

pub(super) fn sort_io_provides(provides: &mut [zzop_core::IoProvide]) {
    provides.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
}

/// Consume-side twin of [`sort_io_provides`] — same `(kind, key, file, line)` order (`IoConsume::key` is
/// `Option<String>`, whose `Ord` sorts `None` before `Some`, a stable and deterministic choice).
pub(super) fn sort_io_consumes(consumes: &mut [zzop_core::IoConsume]) {
    consumes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
}

/// True for the extensions the dispatch table routes to `Language::Python` — mirrors
/// `crate::dead_exports::is_ts_source_ext`'s own "extension-based and duplicated rather than threading
/// the dispatch config" convention (see that function's doc): both `resolve_python_import`'s callers here
/// only ever see `ts_paths` members, whose extension already pins their dispatched language.
pub(in crate::analyze) fn is_python_source_ext(rel: &str) -> bool {
    rel.ends_with(".py") || rel.ends_with(".pyi")
}

/// Python import-specifier resolution glue: wraps `zzop_parser_python_3::python_import_candidates` (a pure
/// candidate builder, no filesystem/tree awareness) with a membership check against `all_paths` — the
/// same known-paths set `ts_paths` already is (see `pipeline::FileArtifact::imports`'s doc for why a
/// Python file lands in that shared, TS-named set). First candidate present in `all_paths` wins,
/// deterministic by the candidate builder's own pinned order.
///
/// **Resolver wiring shape (task 4a/4b)**: `zzop_parser_typescript::resolve::build_dep_impl`
/// (`build_dep_with_workspace`'s private implementation) hardcodes its own resolver closure — there is no
/// parameter to swap in a different resolver for a subset of files, and forking that TS-internal function
/// to add Python awareness would break the swc-isolation-style "one frontend, one resolver" boundary this
/// workspace's crate split maintains. So Python import resolution lives entirely on the ENGINE side,
/// called from two places: [`super::dep_graph::merge_python_dep_edges`] (dep-graph edges, a post-hoc pass
/// run right after `build_dep_with_workspace` returns) and the `compose_router_mount_provides` resolver
/// closure in `super::provides` (cross-file `include_router` mount composition) — both need the identical
/// specifier -> file resolution, just called with a different `original` per call site's own data shape.
/// `package_roots` is the run's resolved `vocabulary.pythonPackageRoots` (empty when undeclared —
/// the built-in tree-root/`src/` roots always apply inside the candidate builder), threaded from each
/// caller's own `config.vocabulary.resolve()` so a declared layout reaches every Python resolution
/// site identically.
pub(in crate::analyze) fn resolve_python_import(
    specifier: &str,
    original: Option<&str>,
    from_file: &str,
    all_paths: &HashSet<String>,
    package_roots: &[&str],
) -> Option<String> {
    zzop_parser_python_3::python_import_candidates(specifier, original, from_file, package_roots)
        .into_iter()
        .find(|c| all_paths.contains(c))
}

/// True for the extension the dispatch table routes to `Language::Rust` — same "duplicated rather than
/// threading the dispatch config" convention `is_python_source_ext` documents.
pub(in crate::analyze) fn is_rust_source_ext(rel: &str) -> bool {
    rel.ends_with(".rs")
}

/// True for the extensions an import PRE-SCAN targets — see
/// `super::collect::Collected::prescan_rels`'s doc for what the pre-scan does with them.
///
/// The roster is NOT spelled here. It belongs to the crate that decides its own reach
/// (`zzop_parser_typescript::PRESCAN_IMPORT_HOSTS`, beside the readers that can actually answer "where
/// does this dialect keep its imports"), and this call site asks it through `prescan_mode`. That is a
/// DEPARTURE from the "duplicated extension check rather than threading the dispatch config"
/// convention `is_python_source_ext` documents, and it is deliberate: the duplicate is what went
/// stale. `.md` joined on 2026-08-20 and `.mdx`/`.astro` on 2026-08-21, each through a different
/// reader, and a literal here would have kept the pre-scan blind to them while the parser crate
/// already handled them unchanged.
///
/// This predicate deliberately collapses the MODE away — it answers "is this file pre-scanned at
/// all", which is the only question the collection gate has. Which reader runs is
/// `extract_prescan_imports`'s business, one layer down, and a mode leaking up to here would be a
/// second table to keep in step.
///
/// These files dispatch to `None` by construction (`crate::dispatch` has no arm for any of them), so
/// there is no `Language` variant to check against.
pub(super) fn is_prescan_ext(rel: &str) -> bool {
    let Some(ext) = std::path::Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
    else {
        return false;
    };
    zzop_parser_typescript::prescan_mode(ext).is_some()
}

mod rust;

pub(in crate::analyze) use rust::resolve_rust_import;
pub(super) use rust::{rust_head, RUST_STD_CRATE_FAMILY};

pub(super) use go::{
    find_go_mount_target, go_fragment_dirs, is_go_source_ext, is_go_std_import, is_go_test_file,
    resolve_go_import_package_dir,
};
