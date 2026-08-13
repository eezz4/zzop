//! `SourceSymbol` extraction — v1 scope: every item this FILE declares, INCLUDING items nested inside an
//! inline `mod foo { ... }` block. A nested item's name is QUALIFIED with the enclosing inline-`mod`
//! chain (`x::inner`, `x::y::Type.method`), so its id is `<rel>#x::inner` — Rust's own path separator,
//! and distinct by construction from a top-level homonym's `<rel>#inner`. `macro_rules!` definitions are
//! never extracted (crate root doc's "Scope note: macros"), and the `mod` declaration itself projects no
//! symbol of its own (`SourceSymbolKind` has no module-shaped variant; `lang::imports` is where a `mod`
//! becomes a fact).
//!
//! ## Why qualification is the whole design, not a naming preference
//! This scope is a CONTRACT the rest of the crate reads, not a local convenience: `lang::calls` must
//! attribute a `RawCall` only to a symbol emitted here, or the call graph gets edges leaving a node
//! nothing declares. Until 2026-08-10 `lang::calls` walked INTO inline `mod` bodies while THIS file did
//! not, and a nested `fn handler` therefore reused the id a TOP-LEVEL `fn handler` in the same file gets
//! — one graph node carrying two unrelated functions' edges, which measurably cleared an unguarded
//! mutating route via a homonym's auth call. That was fixed by NARROWING `lang::calls` to match this
//! file, at a stated cost: every call written inside an inline `mod` became invisible to the call graph.
//!
//! Qualification is the fix that pays neither price. Both files now walk inline `mod` bodies and both
//! build the same qualified id, so a nested `handler` and a top-level `handler` are two distinct nodes
//! and neither borrows the other's edges. `every_from_symbol_is_a_symbol_parse_symbols_emits` (in
//! `lang::calls`'s tests) fails if either side moves without the other; widening or narrowing this scope
//! is therefore still a change to BOTH files.
//!
//! A `#[cfg(test)] mod tests { ... }` block is walked like any other inline `mod`, and that is safe for
//! the same reason: its items are `tests::*`, a namespace no deployed symbol can collide with. The
//! separate question of whether a TEST-gated declaration may serve as EVIDENCE for a rule is not this
//! file's to answer and is not answered by qualification — `lang::extractor_guards` decides it for guard
//! extraction, on its own terms.
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
//! An item's OWN `pub` is read literally, and the enclosing inline `mod`'s visibility is NOT folded in:
//! a `pub fn` inside a private `mod x` reports `exported: true` even though nothing outside the file can
//! name it. Deliberate, and the same coarseness the paragraph above already accepts — folding would
//! require modelling re-exports (`pub use x::inner`) to stay honest, which is a resolution question this
//! frontend has no type layer for.
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
//! traits, but every one of those impls still adds methods to the SAME type. Inside an inline `mod`, the
//! two separators COMPOSE and each keeps its own meaning: `x::Type.method` reads "module `x`, type
//! `Type`, method `method`" and nothing else can produce that shape.
//!
//! ## `trait` associated items: `Trait.member`
//! A `trait` projects its own `Interface` symbol AND one `Trait.member` symbol per associated
//! `fn`/`const` it declares — the same dot-separated shape an `impl` block's members get, with the
//! TRAIT's name on the left. The two cannot collide: `impl Trait for Type` names its members after
//! `Type`, and Rust puts traits and types in ONE namespace, so a `trait Health` and a `struct Health`
//! cannot coexist in a module for `Health.ready` to mean two things.
//!
//! Until 2026-08-13 the walk stopped at the `Interface` symbol, and that was an EXTRACTION-SCOPE gap
//! rather than a leaf hole: a `fn ready(&self) -> bool { … }` written as a trait DEFAULT is executable
//! code that runs unless an impl overrides it, and its body sat inside no symbol's span at all, so no
//! method-scan rule could reach it. Nothing was lost to `drop_outer_spans` — the trait's own span is
//! `None` (below), so these leaves take coverage from nothing and are pure addition.
//!
//! This is NOT the field-initializer case that was deliberately left closed elsewhere. That decision
//! rests on a one-line initializer being unable to HOLD the guard shapes rules veto on, so a span there
//! turns every hardened factory into a false positive. A default body is a block: a veto (`if
//! !self.verified() { return … }`) has somewhere to live in it, and the impl-method bodies that already
//! carry spans are the same shape by construction — an override and the default it replaces are
//! interchangeable at the call site.
//!
//! A body-LESS signature (`fn count(&self) -> usize;`) carries `None`/`None`, which is the span
//! contract's own worked example: there is no region, as opposed to a producer that failed to find one.
//!
//! `exported` for a trait member is the TRAIT's visibility. A trait item has none of its own to read
//! literally — Rust's grammar forbids `pub` there and `syn::TraitItemFn` has no `vis` field — so the
//! choice is between a blanket `false` and the trait's, and the trait is written right here in this
//! file. That is what separates it from the trait-IMPL paragraph below, which stays `false` because its
//! effective visibility depends on a trait that may live in another file and this frontend has no type
//! layer to follow it there.
//!
//! `lang::calls` does NOT walk trait default bodies, so a call written in one is invisible to the call
//! graph. That direction is a MISS, never a wrong edge: the invariant both files keep is that every
//! `from_symbol` names a symbol emitted here, and growing this side cannot break it.
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
//! `zzop_core::SourceSymbol`'s "Body span contract" owns the rule and this crate does not restate it;
//! `emit::fn_span` states the one thing that IS local to Rust — `body_start` is the first ATTRIBUTE's
//! line when one is written, so it may sit above the symbol's `line` (pinned to the `fn` token by a
//! separate convention).
//!
//! Only `Function`-kind symbols get a span — and not every one of them, since a trait's body-less
//! signature is a `Function` that encloses nothing. That is a statement about RUST rather than a gap.
//! Unlike Python — whose `class` body genuinely is a statement
//! list, so `zzop_parser_python_3` computes a range for classes too — a Rust `struct`/`enum`/`union`/
//! `trait` has a FIELD or ASSOCIATED-ITEM list. Projecting a span over one would make
//! `dsl::method_scan::gates::drop_outer_spans` treat a field list as scannable and claim a per-member
//! containment the language does not have, so every non-`Function` symbol here carries
//! `None`/`None` and must keep doing so. An inline `mod` block is the same call for a different reason:
//! its body IS a statement-free item list, and every item in it now projects its OWN leaf, so a
//! container span over it would cover nothing that is not already covered. There is likewise no
//! container span to leave holes IN: an `impl` block projects no symbol and a `trait` projects one with
//! no span, so this crate owes the contract's leaf-completeness half nothing.

mod emit;

use syn::Item;
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::line_of;

use emit::{
    const_symbol, emit_impl, emit_trait, function_symbol, is_exported, plain_symbol, static_symbol,
};

/// Extract this file's symbols — see module doc. Empty on parse failure (never panics). Declaration
/// order preserved, and an inline `mod`'s items follow the `mod` in that order (depth-first).
pub fn parse_symbols(rel: &str, text: &str) -> Vec<SourceSymbol> {
    let Some(file) = crate::parse_file(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in &file.items {
        emit_item(rel, item, &[], &mut out);
    }
    out
}

/// Join an enclosing inline-`mod` chain onto a leaf name — `[] + "f"` -> `"f"`, `["x"] + "f"` ->
/// `"x::f"`. The one place the qualified-id convention is spelled, shared with `lang::calls` through
/// [`qualified_symbol_name`] so the two files cannot drift apart on separator or order.
pub(crate) fn qualify(path: &[String], name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", path.join("::"))
    }
}

/// The final path segment of a `syn::Type`, e.g. `Foo` from `Foo<T>` or from `crate::mod_a::Foo`.
/// `None` for any non-`Type::Path` shape (a reference, tuple, etc.) — those `impl` self-types are out of
/// v1 scope (never guessed at; document via the caller skipping the whole `impl` block).
pub(crate) fn type_leaf_name(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(tp) = ty else { return None };
    tp.path.segments.last().map(|s| s.ident.to_string())
}

fn emit_item(rel: &str, item: &Item, path: &[String], out: &mut Vec<SourceSymbol>) {
    match item {
        Item::Fn(f) => out.push(function_symbol(rel, path, f, is_exported(&f.vis))),
        Item::Struct(s) => out.push(plain_symbol(
            rel,
            qualify(path, &s.ident.to_string()),
            SourceSymbolKind::Class,
            line_of(&s.struct_token),
            is_exported(&s.vis),
        )),
        Item::Enum(e) => out.push(plain_symbol(
            rel,
            qualify(path, &e.ident.to_string()),
            SourceSymbolKind::Class,
            line_of(&e.enum_token),
            is_exported(&e.vis),
        )),
        Item::Union(u) => out.push(plain_symbol(
            rel,
            qualify(path, &u.ident.to_string()),
            SourceSymbolKind::Class,
            line_of(&u.union_token),
            is_exported(&u.vis),
        )),
        // The trait's OWN symbol, then its associated items — the trait is a container that declares
        // members, so stopping at the `Interface` symbol left every default body outside every span.
        Item::Trait(t) => {
            out.push(plain_symbol(
                rel,
                qualify(path, &t.ident.to_string()),
                SourceSymbolKind::Interface,
                line_of(&t.trait_token),
                is_exported(&t.vis),
            ));
            emit_trait(rel, path, t, out);
        }
        Item::Type(t) => out.push(plain_symbol(
            rel,
            qualify(path, &t.ident.to_string()),
            SourceSymbolKind::Type,
            line_of(&t.type_token),
            is_exported(&t.vis),
        )),
        Item::Const(c) => out.push(const_symbol(
            rel,
            qualify(path, &c.ident.to_string()),
            line_of(&c.const_token),
            c,
        )),
        Item::Static(s) => out.push(static_symbol(
            rel,
            qualify(path, &s.ident.to_string()),
            line_of(&s.static_token),
            s,
        )),
        Item::Impl(imp) => emit_impl(rel, path, imp, out),
        // An INLINE `mod x { ... }` (the `content: Some(..)` shape) is this file's own source, so its
        // items are this file's symbols — qualified by `x`, module doc. A `mod x;` DECLARATION
        // (`content: None`) names another FILE and is `lang::imports`' fact, never a symbol here.
        Item::Mod(m) => {
            if let Some((_brace, items)) = &m.content {
                let mut nested = path.to_vec();
                nested.push(m.ident.to_string());
                for inner in items {
                    emit_item(rel, inner, &nested, out);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
