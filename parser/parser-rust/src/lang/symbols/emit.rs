//! `SourceSymbol` SHAPING — split out from `symbols.rs` purely to keep that file under the line-count
//! ratchet. The parent module owns WHAT gets walked (its module doc is the scope contract); this file
//! owns HOW one walked item becomes a `SourceSymbol`. Every naming decision (kind mapping, `exported`,
//! `Type.member`, the span rule) is documented in the parent's module doc and deliberately not restated
//! here.

use syn::{ImplItem, ItemConst, ItemFn, ItemStatic, TraitItem, Visibility};
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::line_of;

use super::{qualify, type_leaf_name};

pub(super) fn is_exported(vis: &Visibility) -> bool {
    !matches!(vis, Visibility::Inherited)
}

/// 1-based END line of a spanned node — `body_end`'s side of the parent module-doc convention (the crate
/// root's shared `line_of` is the START side; kept local because these two body computations are the
/// only END-line consumers in this crate).
fn end_line_of<T: syn::spanned::Spanned>(node: &T) -> u32 {
    node.span().end().line as u32
}

/// The span for an `fn` that has a block: the DECLARATION's own first line through the block's closing
/// brace — `zzop_core::SourceSymbol`'s "Body span contract".
///
/// The declaration's first line is the first ATTRIBUTE when one is written, which is the one place in
/// this repo where `body_start` may sit ABOVE the symbol's `line`. That is deliberate and it is what
/// the contract asks for: `line` is pinned to the `fn` token by a separate, long-standing convention
/// the call graph and the census both read (`line_numbers_are_one_based_and_track_declaration`), while
/// an `#[get("/x")]`/`#[tokio::main]`-anchored method-scan concept is unwritable unless the attribute
/// is inside the span. Nothing consumes the pair as an ordering.
fn fn_span(attrs: &[syn::Attribute], sig: &syn::Signature, block: &syn::Block) -> (u32, u32) {
    let start = attrs
        .first()
        .map(line_of)
        .unwrap_or_else(|| line_of(&sig.fn_token));
    (start, end_line_of(block))
}

pub(super) fn function_symbol(
    rel: &str,
    path: &[String],
    f: &ItemFn,
    exported: bool,
) -> SourceSymbol {
    let name = qualify(path, &f.sig.ident.to_string());
    let line = line_of(&f.sig.fn_token);
    let (start, end) = fn_span(&f.attrs, &f.sig, &f.block);
    SourceSymbol {
        id: format!("{rel}#{name}"),
        file: rel.to_string(),
        exported,
        name,
        kind: SourceSymbolKind::Function,
        line,
        is_default: false,
        body_start: Some(start),
        body_end: Some(end),
        write_sites: Vec::new(),
    }
}

pub(super) fn plain_symbol(
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

pub(super) fn const_symbol(rel: &str, name: String, line: u32, c: &ItemConst) -> SourceSymbol {
    plain_symbol(
        rel,
        name,
        SourceSymbolKind::Const,
        line,
        is_exported(&c.vis),
    )
}

pub(super) fn static_symbol(rel: &str, name: String, line: u32, s: &ItemStatic) -> SourceSymbol {
    plain_symbol(
        rel,
        name,
        SourceSymbolKind::Const,
        line,
        is_exported(&s.vis),
    )
}

/// `trait <Trait>` -> one `Trait.member` symbol per associated `fn`/`const` in the trait's own item
/// list, ALONGSIDE the `Interface` symbol the trait itself projects (parent module doc). A `fn` with a
/// DEFAULT BODY gets that body's span; a bare signature (`fn f(&self);`) gets `None`/`None`, which is
/// the span contract's own example of what `None` claims.
///
/// `exported` is the TRAIT's visibility, not `Inherited` — a trait item has no visibility of its own to
/// read (Rust's grammar forbids `pub` there, so `syn::TraitItemFn` carries no `vis` field at all), and
/// the parent module doc explains why that is a different question from the trait-IMPL case.
///
/// Associated TYPES are deliberately absent, mirroring `emit_impl`: both walks emit `fn` and `const` and
/// nothing else, so the two sides of one trait cannot answer differently about what a member is.
pub(super) fn emit_trait(
    rel: &str,
    path: &[String],
    t: &syn::ItemTrait,
    out: &mut Vec<SourceSymbol>,
) {
    let exported = is_exported(&t.vis);
    let trait_name = t.ident.to_string();
    for item in &t.items {
        match item {
            TraitItem::Fn(f) => {
                let name = qualify(path, &format!("{trait_name}.{}", f.sig.ident));
                let span = f.default.as_ref().map(|b| fn_span(&f.attrs, &f.sig, b));
                out.push(SourceSymbol {
                    id: format!("{rel}#{name}"),
                    file: rel.to_string(),
                    exported,
                    name,
                    kind: SourceSymbolKind::Function,
                    line: line_of(&f.sig.fn_token),
                    is_default: false,
                    body_start: span.map(|(s, _)| s),
                    body_end: span.map(|(_, e)| e),
                    write_sites: Vec::new(),
                });
            }
            TraitItem::Const(c) => {
                let name = qualify(path, &format!("{trait_name}.{}", c.ident));
                let line = line_of(&c.const_token);
                out.push(plain_symbol(
                    rel,
                    name,
                    SourceSymbolKind::Const,
                    line,
                    exported,
                ));
            }
            _ => {}
        }
    }
}

/// `impl <Type>` / `impl <Trait> for <Type>` -> one `Type.member` symbol per associated `fn`/`const`
/// directly in the impl block's own item list (parent module doc). Skipped entirely when `self_ty` isn't
/// a plain path type (`type_leaf_name` returns `None`) — never guessed. `path` is the enclosing inline
/// `mod` chain, so an impl written inside `mod x` yields `x::Type.member`.
pub(super) fn emit_impl(
    rel: &str,
    path: &[String],
    imp: &syn::ItemImpl,
    out: &mut Vec<SourceSymbol>,
) {
    let Some(type_name) = type_leaf_name(&imp.self_ty) else {
        return;
    };
    for item in &imp.items {
        match item {
            ImplItem::Fn(f) => {
                let name = qualify(path, &format!("{type_name}.{}", f.sig.ident));
                let line = line_of(&f.sig.fn_token);
                let (start, end) = fn_span(&f.attrs, &f.sig, &f.block);
                out.push(SourceSymbol {
                    id: format!("{rel}#{name}"),
                    file: rel.to_string(),
                    exported: is_exported(&f.vis),
                    name,
                    kind: SourceSymbolKind::Function,
                    line,
                    is_default: false,
                    body_start: Some(start),
                    body_end: Some(end),
                    write_sites: Vec::new(),
                });
            }
            ImplItem::Const(c) => {
                let name = qualify(path, &format!("{type_name}.{}", c.ident));
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
