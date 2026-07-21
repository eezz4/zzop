//! Small shared helpers used by two or more `assemble` phases: Python, Rust, Go, and Java
//! source-extension / import-resolution glue. Native-analysis timing bookkeeping
//! (`record_native_timing`/`sort_rule_timings`) lives one level up, at `crate::analyze`, since
//! `analyze::native_rules` (a sibling of `assemble`, not a descendant) needs it too.

use std::collections::HashSet;

use crate::pipeline::{GoModuleMap, RustWorkspaceMap};

mod csharp;
mod java;

pub(super) use csharp::{is_csharp_source_ext, is_csharp_std_import, resolve_csharp_import};
pub(super) use java::{
    is_java_source_ext, is_java_std_import, java_census_key, resolve_java_import,
};

/// Deterministic `(kind, key, file, line)` total order for the tree's final IO provide array — applied
/// right before `IoFacts` assembly in `super::assemble` so emitted order is stable across runs regardless
/// of collection order. Its consume-side twin `sort_io_consumes` uses the identical key order.
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
pub(super) fn is_python_source_ext(rel: &str) -> bool {
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
pub(super) fn resolve_python_import(
    specifier: &str,
    original: Option<&str>,
    from_file: &str,
    all_paths: &HashSet<String>,
) -> Option<String> {
    zzop_parser_python_3::python_import_candidates(specifier, original, from_file)
        .into_iter()
        .find(|c| all_paths.contains(c))
}

/// True for the extension the dispatch table routes to `Language::Rust` — same "duplicated rather than
/// threading the dispatch config" convention `is_python_source_ext` documents.
pub(super) fn is_rust_source_ext(rel: &str) -> bool {
    rel.ends_with(".rs")
}

/// True for the extensions the SFC `<script>`-block pre-scan targets (`.vue`/`.svelte`, case-insensitive)
/// — see `super::collect::Collected::sfc_rels`'s doc. Same "duplicated extension check rather than
/// threading the dispatch config" convention `is_python_source_ext` documents, with the extra twist that
/// these files dispatch to `None` by construction (`crate::dispatch` has no `.vue`/`.svelte` arm), so
/// there is no `Language` variant to check against.
pub(super) fn is_sfc_ext(rel: &str) -> bool {
    let Some(ext) = std::path::Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
    else {
        return false;
    };
    matches!(ext.to_ascii_lowercase().as_str(), "vue" | "svelte")
}

/// Rust standard/compiler-provided crate family — never a genuinely external (third-party) package, so a
/// `use std::...`/`use core::...`/... head is excluded from the package-import census entirely (task 4's
/// "exclude the std family from the census" requirement), the same way a Python relative specifier
/// (`starts_with('.')`) never even reaches `resolve_python_import`.
pub(super) const RUST_STD_CRATE_FAMILY: &[&str] = &["std", "core", "alloc", "proc_macro", "test"];

/// The first `::`-segment of a Rust import specifier (`"crate::a::b"` -> `"crate"`, `"serde::Deserialize"`
/// -> `"serde"`, a bare single-segment specifier -> itself unchanged).
pub(super) fn rust_head(specifier: &str) -> &str {
    specifier.split("::").next().unwrap_or(specifier)
}

/// Rust import-specifier resolution glue — the Rust-side counterpart of `resolve_python_import`, unifying
/// TWO resolution paths behind one call: `zzop_parser_rust::rust_import_candidates` (pure, in-tree
/// `crate::`/`super::`/`self::` module-path resolution — returns an empty candidate list for any other
/// head, including a bare external one) tried first, then — only reached when the first path yields
/// nothing, which is always true for an external head since `rust_import_candidates` itself never
/// resolves one — a same-workspace crate lookup via `workspace` (task 6's "dogfooding payoff": an
/// external head like `zzop_core` resolving to `crates/core/src/lib.rs`). Both candidate lists are
/// checked against `all_paths`, first-present-wins, mirroring `resolve_python_import`'s own convention.
/// Called from BOTH [`super::dep_graph::merge_rust_dep_edges`] (dep-graph edges) and the router-mount
/// compose resolver closure in `super::provides` (cross-file `.nest()`/`.merge()` mounts) — same dual-call
/// shape `resolve_python_import`'s own doc describes for its two call sites.
pub(super) fn resolve_rust_import(
    specifier: &str,
    from_file: &str,
    all_paths: &HashSet<String>,
    workspace: &RustWorkspaceMap,
) -> Option<String> {
    let candidates = zzop_parser_rust::rust_import_candidates(specifier, from_file);
    if let Some(hit) = candidates.into_iter().find(|c| all_paths.contains(c)) {
        return Some(hit);
    }
    let head = rust_head(specifier);
    if matches!(head, "crate" | "super" | "self") {
        return None;
    }
    workspace
        .get(head)?
        .iter()
        .find(|c| all_paths.contains(c.as_str()))
        .cloned()
}

/// True for the extension the dispatch table routes to `Language::Go` — same "duplicated rather than
/// threading the dispatch config" convention `is_python_source_ext`/`is_rust_source_ext` document.
pub(super) fn is_go_source_ext(rel: &str) -> bool {
    rel.ends_with(".go")
}

/// True for a Go test file (`foo_test.go`) — Go's own compiler-recognized test-file naming convention.
/// `merge_go_dep_edges`'s target-file filter (task 5's "non-`_test.go`" rule) and
/// `rules_graph::unreachable`'s entry-point recognition both key off this exact suffix.
pub(super) fn is_go_test_file(rel: &str) -> bool {
    rel.strip_suffix(".go")
        .is_some_and(|stem| stem.ends_with("_test"))
}

/// True when `specifier` is a Go standard-library import path — Go's own rule (task 6): the FIRST `/`-
/// segment contains no `.`. A third-party import path always leads with a domain-shaped segment
/// (`github.com/...`, `gopkg.in/...`, containing a `.`); the standard library never does (`fmt`,
/// `net/http`, `encoding/json`). Never censused, never staged for the F5 drain below — the same
/// "excluded before staging" treatment `RUST_STD_CRATE_FAMILY` gives `use std::...`.
pub(super) fn is_go_std_import(specifier: &str) -> bool {
    let first_segment = specifier.split('/').next().unwrap_or(specifier);
    !first_segment.contains('.')
}

/// Go import-specifier resolution glue — the Go-side counterpart of `resolve_rust_import`, but resolving
/// to a PACKAGE DIRECTORY rather than a single file (module doc / `merge_go_dep_edges`'s doc explain why:
/// a Go import path names a package, a directory-wide compilation unit, never one file). Finds `from_file`'s
/// governing module (`pipeline::governing_go_module`'s nearest-`go.mod`-ancestor rule), then resolves
/// `specifier` against that module's own path via `zzop_parser_go::go_package_dir_of`, joining the module
/// root directory back on via `pipeline::go_module_join_dir`. `None` for a file with no governing module,
/// or a `specifier` outside that module's own namespace (a std or third-party import) — the caller then
/// treats it as external (census) rather than guessing an in-tree target. Called from BOTH
/// `super::dep_graph::merge_go_dep_edges` (dep-graph edges) and the census F5 drain in
/// `super::collect::census` — same dual-call shape `resolve_rust_import`'s own doc describes.
pub(super) fn resolve_go_import_package_dir(
    specifier: &str,
    from_file: &str,
    go_modules: &GoModuleMap,
) -> Option<String> {
    let (module_root_dir, module_path) =
        crate::pipeline::governing_go_module(from_file, go_modules)?;
    let remainder_dir = zzop_parser_go::go_package_dir_of(specifier, module_path)?;
    Some(crate::pipeline::go_module_join_dir(
        module_root_dir,
        &remainder_dir,
    ))
}

/// Directory -> that directory's `(file, fragment names)` pairs, restricted to `.go` files carrying at
/// least one router-mount fragment. The substrate `super::provides`'s Go resolver branch searches to
/// disambiguate a package-directory-wide mount: `resolve_go_import_package_dir` above resolves a Go
/// import path to a PACKAGE DIRECTORY (many files), never one file, so picking the ONE file that
/// satisfies a `Mount` needs the mount's own `ident` matched against each candidate file's fragment
/// names — information the generic `resolve(specifier, from_file, ident)` closure signature (shared
/// with every other language branch) carries as `ident`, but the closure has no other way to see
/// fragment names since `router_mount_pairs` is otherwise consumed whole by
/// `compose_router_mount_provides`. Built once by the caller, from a borrow, before that move.
/// Per-directory file lists are sorted by rel path so [`find_go_mount_target`] picks deterministically
/// even in the (unexpected) case of an ident collision across two files in the same directory.
pub(super) fn go_fragment_dirs(
    router_mount_pairs: &[(String, Vec<zzop_core::RouterMountFragment>)],
) -> std::collections::HashMap<String, Vec<(String, Vec<String>)>> {
    let mut by_dir: std::collections::HashMap<String, Vec<(String, Vec<String>)>> =
        std::collections::HashMap::new();
    for (file, frags) in router_mount_pairs {
        if !is_go_source_ext(file) {
            continue;
        }
        let dir = match file.rfind('/') {
            Some(idx) => file[..idx].to_string(),
            None => String::new(),
        };
        let names = frags.iter().map(|f| f.name.clone()).collect();
        by_dir.entry(dir).or_default().push((file.clone(), names));
    }
    for files in by_dir.values_mut() {
        files.sort_by(|a, b| a.0.cmp(&b.0));
    }
    by_dir
}

/// Finds the file, in `dirs`' bucket for `dir`, whose fragment-name set contains `ident` — the
/// `go_fragment_dirs` doc above has the full rationale. `None` when `dir` has no bucket (no
/// router-mount-bearing `.go` file in it) or no bucketed file's fragment set names `ident` — the
/// caller treats this exactly like any other unresolvable mount (conservative: skip the subtree).
pub(super) fn find_go_mount_target<'a>(
    dirs: &'a std::collections::HashMap<String, Vec<(String, Vec<String>)>>,
    dir: &str,
    ident: &str,
) -> Option<&'a str> {
    dirs.get(dir)?
        .iter()
        .find(|(_, names)| names.iter().any(|n| n == ident))
        .map(|(f, _)| f.as_str())
}

#[cfg(test)]
mod go_helper_tests {
    use super::*;

    #[test]
    fn is_go_test_file_matches_only_the_underscore_test_suffix() {
        assert!(is_go_test_file("pkg/handler_test.go"));
        assert!(!is_go_test_file("pkg/handler.go"));
        assert!(!is_go_test_file("pkg/testdata.go"));
    }

    #[test]
    fn is_go_std_import_matches_dotless_first_segments_only() {
        assert!(is_go_std_import("fmt"));
        assert!(is_go_std_import("net/http"));
        assert!(is_go_std_import("encoding/json"));
        assert!(!is_go_std_import("github.com/gin-gonic/gin"));
        assert!(!is_go_std_import("gopkg.in/yaml.v3"));
    }

    #[test]
    fn resolve_go_import_package_dir_resolves_module_root_and_subpackages() {
        let mut modules = GoModuleMap::new();
        modules.insert(String::new(), "example.com/app".to_string());
        assert_eq!(
            resolve_go_import_package_dir("example.com/app", "main.go", &modules),
            Some(String::new())
        );
        assert_eq!(
            resolve_go_import_package_dir("example.com/app/internal/db", "main.go", &modules),
            Some("internal/db".to_string())
        );
    }

    #[test]
    fn resolve_go_import_package_dir_returns_none_for_std_and_external() {
        let mut modules = GoModuleMap::new();
        modules.insert(String::new(), "example.com/app".to_string());
        assert_eq!(
            resolve_go_import_package_dir("fmt", "main.go", &modules),
            None
        );
        assert_eq!(
            resolve_go_import_package_dir("github.com/some/dep", "main.go", &modules),
            None
        );
    }

    #[test]
    fn resolve_go_import_package_dir_returns_none_with_no_governing_module() {
        let modules = GoModuleMap::new();
        assert_eq!(
            resolve_go_import_package_dir("example.com/app", "main.go", &modules),
            None
        );
    }
}
