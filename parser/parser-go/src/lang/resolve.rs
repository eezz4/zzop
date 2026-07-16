//! Pure import-path -> package-directory resolver — the Go-side counterpart of
//! `zzop_parser_rust::lang::resolve::rust_import_candidates` / `python_import_candidates`. No
//! filesystem I/O here; the engine maps the returned directory onto its own known-paths set (same
//! split those two modules' docs describe).
//!
//! Go's own import model makes this dramatically simpler than Rust's `crate`/`super`/`self` anchoring
//! or Python's package/relative-import resolution: an import path is either (a) exactly the module's
//! own path (the module root package) or PREFIXED by `"<module path>/"` (a package one or more
//! directories below the module root — Go's `go.mod`-relative directory layout mirrors the import
//! path one-for-one, no `mod.rs`/`__init__.py`-style indirection to account for), or (b) neither, an
//! import of a different module entirely (a third-party dependency, or the standard library) — always
//! external, never resolved here.
//!
//! The returned directory is TREE-RELATIVE (no leading/trailing slash; `""` for the module root
//! itself) — `go_package_dir_of("github.com/acme/app", "github.com/acme/app")` -> `Some("")`. The
//! caller (the engine) still has to map that directory onto its own known-paths set to find the
//! actual `.go` files living there; this function only answers the pure string question.

/// `import_path` resolved against `module_path` (the `go.mod` `module` directive's own value, no
/// trailing slash) -> the tree-relative package directory, or `None` for a non-matching (external)
/// import — module doc.
pub fn go_package_dir_of(import_path: &str, module_path: &str) -> Option<String> {
    if module_path.is_empty() {
        return None; // no module path to anchor against — never guessed.
    }
    if import_path == module_path {
        return Some(String::new());
    }
    let prefix = format!("{module_path}/");
    import_path
        .strip_prefix(&prefix)
        .filter(|rest| !rest.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests;
