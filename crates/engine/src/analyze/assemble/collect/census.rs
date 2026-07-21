//! F5 census drain — resolves each language's staged (deferred) package-import candidates
//! (`collect()`'s own `python_package_import_candidates`/`rust_package_import_candidates`/
//! `go_package_import_candidates`/`java_package_import_candidates`) against the now-final
//! `ts_paths`/`rust_workspace`/`go_modules`/`java_index`, so an in-tree specifier never pollutes
//! `package_import_files`. Split out from `collect()` purely to keep that function under the line-count
//! ratchet — no new resolution logic lives here, just the drain loops `collect()` used to run inline.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::pipeline::{CSharpIndex, GoModuleMap, JavaIndex, RustWorkspaceMap};

use super::super::helpers::{
    java_census_key, resolve_csharp_import, resolve_go_import_package_dir, resolve_java_import,
    resolve_python_import, resolve_rust_import,
};

/// Python F5 drain: a specifier that resolves in-tree (`app.services` -> `app/services.py`) is
/// first-party — never enters `package_import_files`. A specifier that does NOT resolve (`fastapi`,
/// `requests`) is genuinely external and enters the census exactly as it would without this staging.
pub(super) fn drain_python_candidates(
    candidates: Vec<(String, Option<String>, String)>,
    ts_paths: &HashSet<String>,
    package_import_files: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for (specifier, original, from_file) in candidates {
        if resolve_python_import(&specifier, original.as_deref(), &from_file, ts_paths).is_none() {
            package_import_files
                .entry(specifier)
                .or_default()
                .insert(from_file);
        }
    }
}

/// Rust F5 drain: a head that resolves to a same-workspace crate's root file (`zzop_core` ->
/// `crates/core/src/lib.rs`) is first-party — never enters `package_import_files`. A head that does NOT
/// resolve (`serde`, `axum`) is genuinely external and enters the census.
pub(super) fn drain_rust_candidates(
    candidates: Vec<(String, String)>,
    ts_paths: &HashSet<String>,
    rust_workspace: &RustWorkspaceMap,
    package_import_files: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for (head, from_file) in candidates {
        if resolve_rust_import(&head, &from_file, ts_paths, rust_workspace).is_none() {
            package_import_files
                .entry(head)
                .or_default()
                .insert(from_file);
        }
    }
}

/// Go F5 drain: an import path that resolves in-tree against the importing file's governing module
/// (`example.com/app/internal/db` under `module example.com/app` -> package dir `internal/db`) is
/// first-party — never enters `package_import_files`. A path that does NOT resolve (already known to be
/// dot-shaped / non-std, per `is_go_std_import`'s pre-stage filter in `collect()`) is genuinely external
/// and enters the census.
///
/// Census GRAIN: the FULL import path, verbatim — not trimmed to a "head" the way Rust's `use`
/// specifiers are (`resolve_rust_import`'s own `rust_head` split). A Rust `use` specifier can carry an
/// ITEM path past the crate name (`serde::Deserialize`), so trimming to the crate-naming head is what
/// makes the census one entry per actual dependency; a Go import path never carries anything past the
/// package itself (Go's import statement names ONLY a package, never a symbol inside it — the analogous
/// grain Python's own F5 drain already uses for its dotted absolute specifiers, `drain_python_candidates`
/// above, which also stages/censuses the specifier as-is). So the Go grain matches Python's convention,
/// not Rust's: `github.com/gin-gonic/gin` and `github.com/gin-gonic/gin/binding` (if ever imported) are
/// two distinct census entries, each already naming exactly the dependency-shaped unit a consumer cares
/// about.
pub(super) fn drain_go_candidates(
    candidates: Vec<(String, String)>,
    go_modules: &GoModuleMap,
    package_import_files: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for (specifier, from_file) in candidates {
        if resolve_go_import_package_dir(&specifier, &from_file, go_modules).is_none() {
            package_import_files
                .entry(specifier)
                .or_default()
                .insert(from_file);
        }
    }
}

/// Java F5 drain: an import specifier that resolves in-tree (`resolve_java_import`, non-empty — a plain/
/// static type match, or at least one glob-fanout file) is first-party — never enters
/// `package_import_files`. A specifier that does NOT resolve is genuinely external and enters the census
/// at [`java_census_key`]'s first-two-dotted-segments grain (`helpers`'s own doc for why Java's grain
/// differs from Go's full-path grain) rather than the raw specifier verbatim — deliberately COARSER than
/// every other language's own F5 drain, so `org.springframework.web.bind.annotation.GetMapping` and
/// `org.springframework.web.client.RestTemplate` both collapse into ONE `org.springframework` census
/// entry instead of polluting the census with one row per imported class.
pub(super) fn drain_java_candidates(
    candidates: Vec<(String, String)>,
    java_index: &JavaIndex,
    package_import_files: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for (specifier, from_file) in candidates {
        if resolve_java_import(&specifier, java_index).is_empty() {
            package_import_files
                .entry(java_census_key(&specifier))
                .or_default()
                .insert(from_file);
        }
    }
}

/// C# drain: a `using` specifier that resolves in-tree (`resolve_csharp_import`, non-empty — at least one
/// file declares that namespace) is first-party — never enters `package_import_files`. A specifier that
/// does NOT resolve is genuinely external and enters the census at the raw specifier verbatim (a C#
/// namespace is already a meaningfully-scoped grain on its own — unlike Java's reverse-domain-heavy
/// convention, `helpers::java_census_key`'s doc — so no separate truncation grain is needed here, same
/// full-specifier convention `drain_go_candidates`'s own doc uses for Go's import paths).
pub(super) fn drain_csharp_candidates(
    candidates: Vec<(String, String)>,
    csharp_index: &CSharpIndex,
    package_import_files: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for (specifier, from_file) in candidates {
        if resolve_csharp_import(&specifier, csharp_index).is_empty() {
            package_import_files
                .entry(specifier)
                .or_default()
                .insert(from_file);
        }
    }
}
