//! Import extraction -> `zzop_core::ImportMap`. Scope: top-level `use` items and top-level bodiless
//! `mod x;` declarations only (`syn::File::items`'s direct children; a `use` nested inside a function
//! body is out of scope). A `use` written inside an inline `mod x { ... }` is likewise NOT collected —
//! that is a decided non-capability, not a to-do, and the reason is in the next section.
//!
//! ## Why an inline `mod`'s `use` stays out — the SPECIFIER's anchor, not the map's key
//! The obvious objection is about the KEY: `ImportMap` is keyed by local name, so pouring a nested
//! binding into the file-level map could shadow a top-level one (`use std::fs::File` at top level,
//! `use std::io::Read as File` inside `mod inner`). Measured, that is not the binding constraint — in
//! the shipped corpus exactly 1 of 111 nested bindings names something already bound at file level.
//! Recount for every number below, with its metric definitions:
//! `tests::inline_mod_use_census` (`ZZOP_CENSUS_ROOT=<dir> cargo test -p zzop-parser-rust --lib
//! inline_mod_use_census -- --ignored --nocapture`).
//!
//! The constraint is that a nested specifier's ANCHOR is depth-relative while this map is file-level.
//! `super::` written inside `mod tests { ... }` names THIS FILE's module; `super::` written at file
//! level names the file's PARENT. [`super::resolve::rust_import_candidates`] only ever has the file's
//! anchor (its module doc's "File-layout anchoring" section), so a poured `use super::X` resolves one
//! module too far up — a WRONG edge, and this crate's ledger ranks a wrong edge below a missing one
//! precisely because a missing edge shows up as an island while a misaimed one does not. The same
//! applies to `self::` and to a bodiless `mod y;` nested inside an inline `mod x` (its file is
//! `x/y.rs`, not the sibling `y.rs` the file-level encoding would name). Relative heads are the
//! DOMINANT shape, not an edge case: 119 of 152 nested bindings in this repo's own `crates/`.
//!
//! Qualifying the key instead (`inner::File`, the axis `lang::symbols` picked for nested items) does
//! not rescue it either, and the reason is a contract, not a preference: [`super::calls`]'s `Level`
//! qualifies a callee name only when the enclosing `mod`'s own ITEM LIST declares it (`Level::declared`
//! is built from item idents, never from `use` items). A name reached through a nested `use` therefore
//! stays bare on the callee side, so `zzop_core::callgraph::resolve_name`'s `imports.get(name)` could
//! never hit an `inner::File` key. Two of this crate's own key readers — `adapters::http_clients`'s
//! `reqwest_local_names` and `adapters::axum`'s `.nest`/`.merge` operand lookup — match bare idents for
//! the same reason.
//!
//! What the absolute-headed remainder would buy was measured too, and it is the third leg of the
//! decision: after excluding relative heads and name clashes, the `corpus/oss` join baseline yields
//! 110 bindings in 5 files, all of them the same `pub mod onnx { ... }` whole-file-wrapper idiom in
//! one vendored tree — and `be-axum`, the only corpus tree whose Rust code produces `io.provides`, has
//! ZERO. No finding and no join number can move. **Re-examination trigger**: give
//! `rust_import_candidates` a `super`-depth parameter (that is the actual prerequisite), or a corpus
//! tree lands that declares an HTTP/DB surface inside an inline `mod`.
//!
//! (Historical note, because the stale premise is instructive: this scope used to be justified as
//! "mirrors `lang::symbols`'s top-level only scope". That premise died on 2026-08-11 when
//! `lang::symbols` began walking inline `mod` bodies and qualifying what it finds. The behaviour here
//! is still right, but it had been resting on a sibling that moved.)
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
//! live in this same file). What its BODY declares is a different question, answered above.
//!
//! ## `#[path = "..."]` overrides the file-name convention entirely
//! A bodiless `mod` may carry `#[path = "some/file.rs"]`, and then the module's file is that literal
//! path — the `foo.rs`/`foo/mod.rs` naming convention does not apply at all, so encoding it as
//! `self::x` would send `rust_import_candidates` looking for a file that is not there. The declaration
//! is therefore encoded with a head that cannot collide with any Rust path (`#` cannot begin an
//! identifier): specifier `#path::<literal>`, resolved by [`super::resolve`]'s own arm.
//!
//! **The literal is relative to the DIRECTORY CONTAINING THIS FILE**, per the Rust reference, and that
//! is why the value cannot ride the ordinary `self::` path: for a non-root file `foo.rs` the convention
//! anchors children at `foo/`, but a `#[path]` on a top-level `mod` in that same file anchors at
//! `dirname(foo.rs)`. The two disagree by exactly one segment. (Only top-level `mod` items are walked
//! here — `syn::File::items` — which is precisely the case that rule covers; a `#[path]` on a `mod`
//! nested inside an inline `mod { ... }` block anchors differently and is out of scope, same v1
//! "top-level only" boundary this module already draws.)
//!
//! Measured motivation: this repo has 13 such declarations, and every one of them was a MISSING dep
//! edge — one file that eight parser crates pull in was drawn as an island, and a module compiled under
//! a different name than its file (`#[path = "resolve_tests.rs"] mod tests;`) looked dead to a `grep`
//! for its own file stem.
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

pub use super::resolve::PATH_ATTR_HEAD;

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
                let specifier = match path_attr_value(&m.attrs) {
                    Some(literal) => format!("{PATH_ATTR_HEAD}::{literal}"),
                    None => format!("self::{name}"),
                };
                map.insert(
                    name.clone(),
                    ImportBinding {
                        specifier,
                        original: name,
                        deferred: false,
                        type_only: false,
                    },
                );
            }
            // STOP — the nested `use` items you can see from here are NOT an easy win.
            //
            // `Item::Mod` with a body falls through to `_`, and descending into it to collect its
            // `use` leaves is a ~15-line change that looks obviously correct and is not. The reason
            // is NOT the key collision the local-name keying suggests (measured: 1 of 111 in the
            // shipped corpus). It is that a nested specifier's ANCHOR is depth-relative while this
            // map is file-level: `super::X` written one `mod` deep names THIS file's module, and
            // `super::resolve::rust_import_candidates` — the only resolver this map feeds — can
            // receive nothing but the file's own anchor. Poured, it resolves one module too far up
            // and mints a WRONG edge, which this crate's ledger ranks below a missing one because a
            // missing edge shows as an island and a misaimed one shows as nothing at all. Relative
            // heads are the majority shape (119 of 152 in this repo's own `crates/`), so this is the
            // common case, not the corner. Full argument, the yield that was measured against it,
            // and the two re-examination triggers: this module's doc, first section.
            _ => {}
        }
    }
    map
}

/// The string literal of a `#[path = "..."]` attribute, if this item carries one. Reads only the
/// `path` attribute and only in its `= "literal"` form — the sole form the language accepts here — so a
/// sibling `#[cfg(test)]` on the same `mod` is ignored rather than confused for it (that pairing is the
/// common case, not an edge case). Returns `None` for every other attribute shape, which is what keeps
/// this "never guess": an attribute we do not understand leaves the declaration on the ordinary
/// convention path instead of inventing a target.
fn path_attr_value(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        match &attr.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => Some(s.value()),
                _ => None,
            },
            _ => None,
        }
    })
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
