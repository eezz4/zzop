//! Go module (`go.mod`) manifest scan: maps every `go.mod`-bearing ancestor directory of the walked
//! `.go` files to that manifest's own `module <path>` directive value — the substrate
//! [`crate::analyze::assemble::helpers::resolve_go_import_package_dir`] uses to turn a Go import path
//! into a tree-relative PACKAGE DIRECTORY (never a single file — see that function's own doc for why a
//! Go import targets a whole package, not one file). Mirrors `rust_workspace.rs`'s "read manifest via
//! `root.join(rel)` + `std::fs::read_to_string`, degrade to nothing on any I/O/parse failure" discipline
//! for the Go/`go.mod` side; no `toml`-like dependency exists in this workspace, so
//! [`parse_go_module_path`] is a small line-based parse instead (mirroring
//! `rust_workspace::parse_package_name`'s own visible-literal-only convention).
//!
//! ## Multi-module repos: nearest-ancestor wins
//! A single tree can contain more than one `go.mod` (a multi-module repo, or a vendored/example module
//! nested under the main one). [`GoModuleMap`] stores EVERY `go.mod`-bearing directory found, keyed by
//! that directory itself — resolution for a given file is a separate step
//! ([`governing_go_module`]): walk up from the file's own directory, and the FIRST (nearest) `go.mod`
//! directory encountered is that file's governing module. A file under a nested module's tree is never
//! governed by an outer/enclosing module's `go.mod` — same "closest wins" rule `go build` itself applies
//! when locating a file's module.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

/// `go.mod`'s own directory (tree-relative, `""` for the tree root) -> that manifest's `module <path>`
/// directive value. Every distinct `go.mod`-bearing ancestor directory of the walked `.go` files gets an
/// entry; a directory with no `go.mod`, or whose `go.mod` has no (or an unparseable) `module` directive,
/// contributes nothing. See module doc for the multi-module "nearest-ancestor wins" resolution this map
/// alone does not encode (that is [`governing_go_module`]'s job, applied per file at resolution time).
pub(crate) type GoModuleMap = BTreeMap<String, String>;

/// Scans every distinct ancestor directory (deduplicated, deterministic via `BTreeSet`) of
/// `walked_go_files` for a `go.mod`, and maps that directory to its manifest's `module <path>` value.
/// Any I/O failure or missing/unparseable `module` directive degrades to "contributes nothing" for that
/// directory — never panics, same discipline `scan_rust_workspace` upholds for its own manifest reads.
pub(crate) fn scan_go_modules<'a>(
    root: &Path,
    walked_go_files: impl Iterator<Item = &'a str>,
) -> GoModuleMap {
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    for rel in walked_go_files {
        for ancestor in ancestor_dirs(rel) {
            dirs.insert(ancestor);
        }
    }
    let mut map = GoModuleMap::new();
    for dir in dirs {
        let manifest_rel = join_dir(&dir, "go.mod");
        let Ok(text) = std::fs::read_to_string(root.join(&manifest_rel)) else {
            continue;
        };
        let Some(module_path) = parse_go_module_path(&text) else {
            continue;
        };
        map.insert(dir, module_path);
    }
    map
}

/// Walks up from `rel`'s own directory to the tree root, returning the FIRST (nearest) `go.mod`
/// directory found in `modules` and its module path — module doc's "nearest-ancestor wins". `None` when
/// no ancestor directory (including the tree root itself, `""`) has a scanned `go.mod` entry at all —
/// the file has no governing module, so any import in it is unresolvable in-tree (never guessed; the
/// caller treats this exactly like an external import).
pub(crate) fn governing_go_module<'a>(
    rel: &str,
    modules: &'a GoModuleMap,
) -> Option<(&'a str, &'a str)> {
    let mut dir = dirname(rel).to_string();
    loop {
        if let Some((k, v)) = modules.get_key_value(dir.as_str()) {
            return Some((k.as_str(), v.as_str()));
        }
        if dir.is_empty() {
            return None;
        }
        dir = match dir.rfind('/') {
            Some(idx) => dir[..idx].to_string(),
            None => String::new(),
        };
    }
}

/// Every ancestor directory of `rel` (its own dirname, then each parent up to and including the tree
/// root `""`), most specific first — same shape as `rust_workspace::ancestor_dirs`, duplicated locally
/// rather than shared (that function is a private helper with no public home, same reasoning
/// `dispatch.rs`'s own `matches_glob` doc gives for not importing a sibling module's private helper).
fn ancestor_dirs(rel: &str) -> Vec<String> {
    let mut dir = dirname(rel).to_string();
    let mut out = vec![dir.clone()];
    while let Some(idx) = dir.rfind('/') {
        dir.truncate(idx);
        out.push(dir.clone());
    }
    out.push(String::new());
    out
}

fn dirname(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[..i],
        None => "",
    }
}

/// `dir` joined with `seg`, tolerating an empty `dir` (tree root) OR an empty `seg` (the module-root
/// package itself, `go_package_dir_of`'s own `Some("")` case) — unlike `rust_workspace::join`, which
/// never sees an empty `seg`, `analyze::assemble::helpers::resolve_go_import_package_dir` calls this
/// with `seg = ""` whenever an import resolves to its module's ROOT package, and that call must yield
/// `dir` unchanged, not `"dir/"`.
pub(crate) fn join_dir(dir: &str, seg: &str) -> String {
    if seg.is_empty() {
        dir.to_string()
    } else if dir.is_empty() {
        seg.to_string()
    } else {
        format!("{dir}/{seg}")
    }
}

/// Extracts the `go.mod` `module` directive's own path value — a visible-literal-only, line-based parse
/// (no dependency on any `go.mod`-parsing crate). Tolerates leading/trailing whitespace, a commented-out
/// line (`// module ...`, ignored entirely), and a trailing line comment on the directive itself
/// (`module example.com/app // the module`). The directive keyword must be the line's OWN first
/// whitespace-separated token — a `module` appearing elsewhere (a `require`/`replace` block entry, a
/// dependency literally named "module") never matches, mirroring `parse_package_name`'s
/// section-boundary-aware discipline (`go.mod` has no bracketed sections, so the equivalent guard here is
/// simply "first token on the line").
pub(crate) fn parse_go_module_path(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        if parts.next() != Some("module") {
            continue;
        }
        let Some(rest) = parts.next() else {
            continue;
        };
        let path = rest.split("//").next().unwrap_or(rest).trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests;
