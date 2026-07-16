//! Top-level `SourceSymbol` extraction — v1 scope: this FILE's own top-level items only
//! (`syn::File::items`'s direct children; an item nested inside an inline `mod foo { ... }` block is out
//! of scope, mirroring `zzop_parser_python_3::lang::symbols`'s identical "top-level only" v1 scope for a
//! nested `def`/`class`). `macro_rules!` definitions are never extracted (crate root doc's "Scope note:
//! macros").
//!
//! ## `exported`
//! `true` for ANY `pub` spelling — `pub`, `pub(crate)`, `pub(super)`, `pub(in ...)` — `false` only for
//! the fully-private `syn::Visibility::Inherited` (no keyword at all). Rationale: zzop's `exported`
//! means "importable by another in-tree file", and every `pub*` form satisfies that WITHIN a crate
//! (`pub(crate)`/`pub(super)`/`pub(in path)` are all visible to at least one other file elsewhere in the
//! same tree); only a fully private item can never cross a file boundary. This intentionally does not
//! distinguish "visible everywhere" from "visible in a sub-tree" — a coarser signal than `rustc`'s own
//! visibility lattice, but the same granularity `SourceSymbol::exported` offers every other language in
//! this workspace (a single bool).
//!
//! ## Kind mapping (judgment calls)
//! `SourceSymbolKind` has no Rust-shaped variants (it was designed for JS/Python), so each Rust item
//! kind is mapped onto the CLOSEST existing variant:
//! - `fn` (top-level and `impl`-block associated fn) -> `Function`.
//! - `struct` / `enum` / `union` -> `Class` — every one of these is a nominal, fielded/varianted data
//!   type that CAN carry `impl`-block methods, the same shape a TS/Python `class` has (as opposed to a
//!   pure structural contract, which Rust's `trait` is closer to).
//! - `trait` -> `Interface` — a structural behavior contract with no data of its own, mirroring how
//!   `zzop_parser_typescript` maps a TS `interface`.
//! - `type` alias -> `Type`, mirroring a TS type alias.
//! - `const` / `static` (top-level and `impl`-block associated const) -> `Const`.
//!
//! ## `impl` block methods and associated consts: `Type.member`
//! Every `fn`/`const` inside an `impl <Type>` or `impl <Trait> for <Type>` block is emitted as a
//! `Function`/`Const` symbol named `"Type.member"` — the same dot-separated convention
//! `zzop_parser_python_3::lang::symbols` uses for `Class.method` (itself borrowed from the TS
//! `Class.method` convention `lib.rs`'s module doc pins). For `impl Trait for Type`, `Type` (the
//! `self_ty`, i.e. the type AFTER `for`) is used, never `Trait` — an impl'd type can implement many
//! traits, but every one of those impls still adds methods to the SAME type.
//!
//! A trait impl's own methods carry NO visibility keyword of their own (Rust's grammar forbids writing
//! `pub` on a trait-impl item; its effective visibility is inherited from the trait/type, not written).
//! This crate does not attempt to infer that effective visibility — a trait-impl method's
//! `syn::Visibility` always parses as `Inherited`, so it is always `exported: false` here. This is a
//! known, documented judgment call: a trait impl of a `pub` trait for a `pub` type IS in practice
//! reachable from another file (via the trait), but this crate would report `exported: false` for its
//! methods, same coarse-signal tradeoff `exported`'s doc above already accepts elsewhere.
//!
//! ## `body_start`/`body_end`
//! Only `Function`-kind symbols (top-level fns and `impl`-block methods) get a body line range, computed
//! from their block's first/last statement — mirroring the FUNCTION side of
//! `zzop_parser_python_3::lang::symbols`'s convention. Unlike Python (whose `class` body is a statement
//! list, so `zzop_parser_python_3` computes a body range for classes too), a Rust struct/enum/union/trait
//! has a FIELD or ASSOCIATED-ITEM list, not a statement body — there is no statement-shaped range to
//! report, so every non-`Function` symbol here always carries `body_start: None, body_end: None`.

use syn::{ImplItem, Item, ItemConst, ItemFn, ItemStatic, Visibility};
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::line_of;

/// Extract this file's top-level symbols — see module doc. Empty on parse failure (never panics).
/// Declaration order preserved.
pub fn parse_symbols(rel: &str, text: &str) -> Vec<SourceSymbol> {
    let Some(file) = crate::parse_file(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in &file.items {
        emit_item(rel, item, &mut out);
    }
    out
}

fn is_exported(vis: &Visibility) -> bool {
    !matches!(vis, Visibility::Inherited)
}

/// The final path segment of a `syn::Type`, e.g. `Foo` from `Foo<T>` or from `crate::mod_a::Foo`.
/// `None` for any non-`Type::Path` shape (a reference, tuple, etc.) — those `impl` self-types are out of
/// v1 scope (never guessed at; document via the caller skipping the whole `impl` block).
fn type_leaf_name(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(tp) = ty else { return None };
    tp.path.segments.last().map(|s| s.ident.to_string())
}

fn emit_item(rel: &str, item: &Item, out: &mut Vec<SourceSymbol>) {
    match item {
        Item::Fn(f) => out.push(function_symbol(
            rel,
            f.sig.ident.to_string(),
            f,
            is_exported(&f.vis),
        )),
        Item::Struct(s) => out.push(plain_symbol(
            rel,
            s.ident.to_string(),
            SourceSymbolKind::Class,
            line_of(&s.struct_token),
            is_exported(&s.vis),
        )),
        Item::Enum(e) => out.push(plain_symbol(
            rel,
            e.ident.to_string(),
            SourceSymbolKind::Class,
            line_of(&e.enum_token),
            is_exported(&e.vis),
        )),
        Item::Union(u) => out.push(plain_symbol(
            rel,
            u.ident.to_string(),
            SourceSymbolKind::Class,
            line_of(&u.union_token),
            is_exported(&u.vis),
        )),
        Item::Trait(t) => out.push(plain_symbol(
            rel,
            t.ident.to_string(),
            SourceSymbolKind::Interface,
            line_of(&t.trait_token),
            is_exported(&t.vis),
        )),
        Item::Type(t) => out.push(plain_symbol(
            rel,
            t.ident.to_string(),
            SourceSymbolKind::Type,
            line_of(&t.type_token),
            is_exported(&t.vis),
        )),
        Item::Const(c) => out.push(const_symbol(
            rel,
            c.ident.to_string(),
            line_of(&c.const_token),
            c,
        )),
        Item::Static(s) => out.push(static_symbol(
            rel,
            s.ident.to_string(),
            line_of(&s.static_token),
            s,
        )),
        Item::Impl(imp) => emit_impl(rel, imp, out),
        _ => {}
    }
}

fn function_symbol(rel: &str, name: String, f: &ItemFn, exported: bool) -> SourceSymbol {
    let line = line_of(&f.sig.fn_token);
    let body_start = f.block.stmts.first().map(line_of);
    let body_end = f.block.stmts.last().map(line_of);
    SourceSymbol {
        id: format!("{rel}#{name}"),
        file: rel.to_string(),
        exported,
        name,
        kind: SourceSymbolKind::Function,
        line,
        is_default: false,
        body_start,
        body_end,
        write_sites: Vec::new(),
    }
}

fn plain_symbol(
    rel: &str,
    name: String,
    kind: SourceSymbolKind,
    line: u32,
    exported: bool,
) -> SourceSymbol {
    SourceSymbol {
        id: format!("{rel}#{name}"),
        file: rel.to_string(),
        exported,
        name,
        kind,
        line,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    }
}

fn const_symbol(rel: &str, name: String, line: u32, c: &ItemConst) -> SourceSymbol {
    plain_symbol(
        rel,
        name,
        SourceSymbolKind::Const,
        line,
        is_exported(&c.vis),
    )
}

fn static_symbol(rel: &str, name: String, line: u32, s: &ItemStatic) -> SourceSymbol {
    plain_symbol(
        rel,
        name,
        SourceSymbolKind::Const,
        line,
        is_exported(&s.vis),
    )
}

/// `impl <Type>` / `impl <Trait> for <Type>` -> one `Type.member` symbol per associated `fn`/`const`
/// directly in the impl block's own item list (module doc). Skipped entirely when `self_ty` isn't a
/// plain path type (`type_leaf_name` returns `None`) — never guessed.
fn emit_impl(rel: &str, imp: &syn::ItemImpl, out: &mut Vec<SourceSymbol>) {
    let Some(type_name) = type_leaf_name(&imp.self_ty) else {
        return;
    };
    for item in &imp.items {
        match item {
            ImplItem::Fn(f) => {
                let name = format!("{type_name}.{}", f.sig.ident);
                let line = line_of(&f.sig.fn_token);
                let body_start = f.block.stmts.first().map(line_of);
                let body_end = f.block.stmts.last().map(line_of);
                out.push(SourceSymbol {
                    id: format!("{rel}#{name}"),
                    file: rel.to_string(),
                    exported: is_exported(&f.vis),
                    name,
                    kind: SourceSymbolKind::Function,
                    line,
                    is_default: false,
                    body_start,
                    body_end,
                    write_sites: Vec::new(),
                });
            }
            ImplItem::Const(c) => {
                let name = format!("{type_name}.{}", c.ident);
                let line = line_of(&c.const_token);
                out.push(plain_symbol(
                    rel,
                    name,
                    SourceSymbolKind::Const,
                    line,
                    is_exported(&c.vis),
                ));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
