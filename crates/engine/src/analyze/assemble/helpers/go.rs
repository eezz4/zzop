//! Go's half of the assemble-phase language glue — extension/test-file recognition, std-import
//! classification, and the module-map-driven import resolution the dep graph and the fragment scan both
//! read. A sibling of `csharp`/`java`/`rust` for the same reason those exist: one language's
//! conventions in one file, so the parent stays a directory of languages rather than a pile of them.

use crate::pipeline::GoModuleMap;

/// True for the extension the dispatch table routes to `Language::Go` — same "duplicated rather than
/// threading the dispatch config" convention `is_python_source_ext`/`is_rust_source_ext` document.
pub(in crate::analyze::assemble) fn is_go_source_ext(rel: &str) -> bool {
    rel.ends_with(".go")
}

/// True for a Go test file (`foo_test.go`) — Go's own compiler-recognized test-file naming convention.
/// `merge_go_dep_edges`'s target-file filter (task 5's "non-`_test.go`" rule) and
/// `rules_graph::unreachable`'s entry-point recognition both key off this exact suffix.
pub(in crate::analyze::assemble) fn is_go_test_file(rel: &str) -> bool {
    rel.strip_suffix(".go")
        .is_some_and(|stem| stem.ends_with("_test"))
}

/// True when `specifier` is a Go standard-library import path — Go's own rule (task 6): the FIRST `/`-
/// segment contains no `.`. A third-party import path always leads with a domain-shaped segment
/// (`github.com/...`, `gopkg.in/...`, containing a `.`); the standard library never does (`fmt`,
/// `net/http`, `encoding/json`). Never censused, never staged for the F5 drain below — the same
/// "excluded before staging" treatment `RUST_STD_CRATE_FAMILY` gives `use std::...`.
pub(in crate::analyze::assemble) fn is_go_std_import(specifier: &str) -> bool {
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
pub(in crate::analyze::assemble) fn resolve_go_import_package_dir(
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
/// A BTreeMap (not a HashMap) so the bucket walk is ordered by nature; per-directory file lists are
/// sorted by rel path on top of that, so [`find_go_mount_target`] picks deterministically
/// even in the (unexpected) case of an ident collision across two files in the same directory.
pub(in crate::analyze::assemble) fn go_fragment_dirs(
    router_mount_pairs: &[(String, Vec<zzop_core::RouterMountFragment>)],
) -> std::collections::BTreeMap<String, Vec<(String, Vec<String>)>> {
    let mut by_dir: std::collections::BTreeMap<String, Vec<(String, Vec<String>)>> =
        std::collections::BTreeMap::new();
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
pub(in crate::analyze::assemble) fn find_go_mount_target<'a>(
    dirs: &'a std::collections::BTreeMap<String, Vec<(String, Vec<String>)>>,
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
