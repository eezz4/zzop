//! Import extraction -> `zzop_core::ImportMap`. v1 scope: top-level `use` items and top-level bodiless
//! `mod x;` declarations only (`syn::File::items`'s direct children — mirrors `lang::symbols`'s
//! "top-level only" scope; a `use` nested inside a function body is out of scope).
//!
//! ## Specifier convention (`rust_import_candidates` depends on this exactly)
//! `specifier` is the FULL colon-separated path as written, head keyword included verbatim when
//! present: `crate::a::b`, `super::a`, `self::a`, or a bare external head (`serde::Deserialize`,
//! `tokio::spawn`). Unlike `zzop_parser_python_3::lang::imports` (which splits a `from` import into a
//! `specifier` module path and a separate `original` imported-name field), this crate's
//! `rust_import_candidates` takes ONLY `specifier` — no companion "imported name" parameter — so the
//! full path, including its final (possibly item-not-module) segment, must live inside `specifier`
//! itself. `resolve::rust_import_candidates` is the piece that resolves the "is the last segment a
//! module or an item declared inside its parent's module file?" ambiguity, the same question
//! `python_import_candidates` answers via its separate `original` parameter — see that module's doc.
//!
//! `original` still gets a value (the last written path segment) for structural parity with
//! `ImportBinding`'s Python-side usage, but `rust_import_candidates` does not read it.
//!
//! ## `mod x;` declarations
//! A bodiless `mod x;` binds the local name `x` (this file's own submodule) and is encoded with
//! specifier `"self::x"` — reusing the `self::` resolution path verbatim rather than inventing a
//! separate `"mod:x"` scheme, because `mod x;` and `use self::x` name the EXACT SAME location (a child
//! module declared in the current file): both are "the child module `x` of whatever module this file
//! represents", so `resolve::rust_import_candidates`'s `self::`-anchoring logic (crate root doc's "Line
//! numbers" sibling section — see `resolve`'s own module doc) applies identically to either origin. A
//! `mod x { ... }` WITH a body is not an import edge at all (nothing to resolve — the module's contents
//! live in this same file, out of `lang::symbols`' v1 walked scope regardless).
//!
//! ## `pub use` re-exports
//! `zzop_core::ir` models a re-export via a SEPARATE `ReExport` type, but this crate's public API
//! (`parse_rust`) has no re-export output slot, and `ImportBinding` itself carries no re-export flag —
//! so a `pub use` is recorded as an ORDINARY `ImportBinding` edge here (the visibility of the `use` item
//! itself is dropped). This still satisfies "a `pub use` is a real use edge" for dependency-graph
//! purposes; it just does not separately flag the edge as re-exported the way a full `ReExport`
//! consumer would want. Documented judgment call, not an oversight.
//!
//! `deferred`/`type_only` are always `false` — Rust `use` has neither a lazy-import nor an
//! erased-at-compile-time-type-only concept the way JS/TS do.

use syn::{Item, UseTree};
use zzop_core::{ImportBinding, ImportMap};

/// Extract this file's import bindings — see module doc. Empty on parse failure (never panics).
pub fn parse_imports(text: &str) -> ImportMap {
    let mut map = ImportMap::new();
    let Some(file) = crate::parse_file(text) else {
        return map;
    };
    let mut glob_seq: u32 = 0;
    for item in &file.items {
        match item {
            Item::Use(u) => walk_use_tree(&u.tree, &[], &mut map, &mut glob_seq),
            Item::Mod(m) if m.content.is_none() => {
                let name = m.ident.to_string();
                map.insert(
                    name.clone(),
                    ImportBinding {
                        specifier: format!("self::{name}"),
                        original: name,
                        deferred: false,
                        type_only: false,
                    },
                );
            }
            _ => {}
        }
    }
    map
}

/// Recursively walks one `use` tree, threading the path PREFIX (segments seen so far, e.g. `["crate",
/// "a"]` while descending into `crate::a::{b, c as d}`) down to each leaf. A `Group` branches with the
/// SAME prefix for every member; a `Path` segment extends the prefix by one and recurses; a
/// `Name`/`Rename`/`Glob` is a leaf that inserts exactly one `ImportMap` entry.
fn walk_use_tree(tree: &UseTree, prefix: &[String], map: &mut ImportMap, glob_seq: &mut u32) {
    match tree {
        UseTree::Path(p) => {
            let mut next = prefix.to_vec();
            next.push(p.ident.to_string());
            walk_use_tree(&p.tree, &next, map, glob_seq);
        }
        UseTree::Group(g) => {
            for sub in &g.items {
                walk_use_tree(sub, prefix, map, glob_seq);
            }
        }
        UseTree::Name(n) => {
            let seg = n.ident.to_string();
            let specifier = joined(prefix, &seg);
            map.insert(
                seg.clone(),
                ImportBinding {
                    specifier,
                    original: seg,
                    deferred: false,
                    type_only: false,
                },
            );
        }
        UseTree::Rename(r) => {
            let orig = r.ident.to_string();
            let specifier = joined(prefix, &orig);
            let local = r.rename.to_string();
            map.insert(
                local,
                ImportBinding {
                    specifier,
                    original: orig,
                    deferred: false,
                    type_only: false,
                },
            );
        }
        UseTree::Glob(_) => {
            let specifier = prefix.join("::");
            insert_glob(map, glob_seq, specifier);
        }
    }
}

fn joined(prefix: &[String], last: &str) -> String {
    if prefix.is_empty() {
        last.to_string()
    } else {
        format!("{}::{last}", prefix.join("::"))
    }
}

/// A glob import (`use a::b::*;`) binds no single local name — mirrors
/// `zzop_parser_python_3::lang::imports::insert_star`'s synthetic, collision-free map key so the edge
/// still enters the map instead of being silently dropped.
fn insert_glob(map: &mut ImportMap, glob_seq: &mut u32, specifier: String) {
    map.insert(
        format!("__glob_import_{}__", *glob_seq),
        ImportBinding {
            specifier,
            original: "*".to_string(),
            deferred: false,
            type_only: false,
        },
    );
    *glob_seq += 1;
}

#[cfg(test)]
mod tests;
