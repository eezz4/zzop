//! Rust workspace-member manifest scan: maps an external `use` head's crate name — both the manifest's
//! own (possibly hyphenated) `[package] name` and its underscore-normalized `use`-path spelling — to that
//! crate's own root-file candidates, so a same-workspace crate import resolves to a real dep-graph edge
//! instead of landing in the external package-import census (the "dogfooding payoff": `zzop_core` ->
//! `crates/core/src/lib.rs`). Mirrors `package_json_entries`'s "read manifest via `root.join(rel)` +
//! `std::fs::read_to_string`" precedent (see that function's own doc) for the Cargo/`.rs` side; no `toml`
//! dependency exists in this workspace, so [`parse_package_name`] is a small line-based parse instead.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Everything one pass over the tree's `Cargo.toml` files yields. The two products travel together
/// because they are read from the SAME manifests in the same walk, and because import resolution needs
/// both at once: [`Self::crate_roots`] answers "which file is crate `foo`", [`Self::target_roots`]
/// answers "is THIS file a crate root". Splitting them cost two identical directory walks and two reads
/// of every manifest until 2026-08-06, and — worse — let the second product reach only one of the four
/// consumers of the first.
#[derive(Debug, Default)]
pub(crate) struct RustWorkspaceMap {
    crates: BTreeMap<String, Vec<String>>,
    target_roots: BTreeSet<String>,
}

impl RustWorkspaceMap {
    /// Ordered root-file candidates (`src/lib.rs` before `src/main.rs`) for a `use` head naming a
    /// same-workspace crate — both the manifest's own (possibly hyphenated) `[package] name` and its
    /// underscore-normalized `use`-path spelling are keys. Mirrors `resolve_python_import`'s
    /// "candidates, first-present-in-known-paths wins" convention; existence against the tree's own
    /// known-path set is checked by the caller, not here (the split `rust_import_candidates` upholds).
    pub(crate) fn crate_roots(&self, head: &str) -> Option<&Vec<String>> {
        self.crates.get(head)
    }

    /// Files a manifest declares as explicit build targets — `path = "..."` under `[lib]`, `[[bin]]`,
    /// `[[test]]`, `[[example]]`, `[[bench]]` — resolved tree-relative. Two consumers, and they want it
    /// for the same underlying reason (cargo loads these directly, so they are crate ROOTS that nothing
    /// imports): `rust_import_candidates` anchors their child modules as SIBLINGS rather than by
    /// basename convention, and `unreachable_findings` takes them as `extra_entries` so a positive
    /// `fan_in` on one does not read as island signal.
    ///
    /// Auto-discovered targets (`src/bin/`, `tests/`, `examples/`, `benches/`) are deliberately absent:
    /// path convention already identifies those, and this handles only the explicit-`path` escape hatch
    /// convention cannot see.
    pub(crate) fn target_roots(&self) -> &BTreeSet<String> {
        &self.target_roots
    }
}

/// Scans every distinct ancestor directory (deduplicated, deterministic via `BTreeSet`) of
/// `walked_rust_files` for a `Cargo.toml` and reads both products out of it. A directory with no
/// `Cargo.toml`, and any I/O failure, degrade to "contributes nothing" — never panics, same discipline
/// `package_json_entries` upholds for its own manifest reads.
///
/// A manifest with no `[package] name` (a virtual workspace root) contributes no crate mapping but can
/// still contribute target roots, so the two extractions are independent rather than sharing an early
/// return.
pub(crate) fn scan_rust_workspace<'a>(
    root: &Path,
    walked_rust_files: impl Iterator<Item = &'a str>,
) -> RustWorkspaceMap {
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    for rel in walked_rust_files {
        for ancestor in ancestor_dirs(rel) {
            dirs.insert(ancestor);
        }
    }
    let mut out = RustWorkspaceMap::default();
    for dir in dirs {
        let manifest_rel = join(&dir, "Cargo.toml");
        let Ok(text) = std::fs::read_to_string(root.join(&manifest_rel)) else {
            continue;
        };
        for target_path in parse_target_paths(&text) {
            out.target_roots.insert(join(&dir, &target_path));
        }
        let Some(name) = parse_package_name(&text) else {
            continue;
        };
        let candidates = vec![join(&dir, "src/lib.rs"), join(&dir, "src/main.rs")];
        let normalized = name.replace('-', "_");
        out.crates.entry(name).or_insert_with(|| candidates.clone());
        out.crates.entry(normalized).or_insert(candidates);
    }
    out
}

/// The `path = "..."` values under target-shaped section headers (`[lib]`, `[[bin]]`, `[[test]]`,
/// `[[example]]`, `[[bench]]`) — the same section-boundary-aware, line-based parse discipline as
/// [`parse_package_name`] (a `path =` under `[dependencies]` or `[package]` never matches).
pub(crate) fn parse_target_paths(text: &str) -> Vec<String> {
    const TARGET_SECTIONS: &[&str] = &["[lib]", "[[bin]]", "[[test]]", "[[example]]", "[[bench]]"];
    let mut in_target = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_target = TARGET_SECTIONS.contains(&trimmed);
            continue;
        }
        if !in_target || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            if key.trim() == "path" {
                if let Some(v) = parse_quoted(value) {
                    out.push(v);
                }
            }
        }
    }
    out
}

/// Every ancestor directory of `rel` (its own dirname, then each parent up to and including the tree
/// root `""`), most specific first. The caller dedups via a `BTreeSet`, so the trailing `""` this always
/// pushes (even when already present) is harmless.
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

fn join(dir: &str, seg: &str) -> String {
    if dir.is_empty() {
        seg.to_string()
    } else {
        format!("{dir}/{seg}")
    }
}

/// Extracts the `[package]` section's `name = "..."` value — a visible-literal-only, line-based parse (no
/// `toml` dependency in this workspace). Section-boundary aware: only lines between a `[package]` header
/// and the next `[...]`/`[[...]]` header (or EOF) are considered, so a `name =` under `[dependencies]` or
/// `[[bin]]` never matches. Tolerates surrounding whitespace and a trailing comment.
pub(crate) fn parse_package_name(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            if key.trim() == "name" {
                if let Some(v) = parse_quoted(value) {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// The first quoted (single or double) literal in `s`, ignoring any trailing content (a comment, e.g.).
fn parse_quoted(s: &str) -> Option<String> {
    let s = s.trim();
    let s = s.strip_prefix('"').or_else(|| s.strip_prefix('\''))?;
    let end = s.find(['"', '\''])?;
    Some(s[..end].to_string())
}

#[cfg(test)]
mod tests;
