//! Rust call-site extraction — `RawCall` per call expression, the fact the whole-repo symbol call graph
//! is built from.
//!
//! # Why this exists, and why the corpus argument reversed
//! Rust routes to a real structural frontend here but produced ZERO call sites until 2026-07-28, so
//! `CALL_GRAPH_COVERED_EXTENSIONS` had no `rs` and every call-graph-shaped rule was provably inert on
//! Rust trees. The reason it stayed unbuilt was a rule this repo keeps on purpose — no pre-emptive
//! language work without a corpus that can anchor a true positive — and the corpus was one 12-file
//! checkout.
//!
//! That argument died on its own terms: **zzop is itself a large, real Rust workspace, and we know its
//! answers.** An anchor we can check by reading is better than a third-party checkout we cannot.
//!
//! # Scope, stated as a boundary rather than a promise
//! This emits **name-level** calls, exactly like the TypeScript extractor: the callee is an identifier,
//! and turning it into an edge is `zzop_core::callgraph::resolve_calls_for_file`'s job using the file's
//! `ImportMap`. Consequences worth naming, because each is a silent miss otherwise:
//!
//! - **Trait dispatch is not resolved.** `x.run()` where `x: &dyn Runner` yields `callee_name = "run"`
//!   with no receiver type, so it can only ever resolve to a same-file or imported `run`. Rust's
//!   monomorphized dispatch is a type-layer fact and this frontend has no type layer (ledger R2).
//! - **Macro bodies are not walked.** `syn` parses a macro invocation as an opaque token stream, so a
//!   call written inside `println!`/`vec!`/a derive is invisible. This is the same class as the
//!   TypeScript extractor's blindness to calls inside template literals.
//! - **Closures ARE walked**, and their calls are attributed to the enclosing named symbol. A closure has
//!   no symbol id of its own, and attributing to the enclosing function is what makes a handler's
//!   reachability BFS work.
//! - **`Type::assoc()` sets `receiver_type`** from the path's second-to-last segment, which is what lets
//!   a cross-file `<file>#<Type>.<assoc>` edge resolve. `self.method()` deliberately does NOT set one:
//!   `Self` is not an importable name, and guessing the impl's type here would produce an edge the
//!   resolver cannot verify.
//!
//! # Inline `mod` bodies ARE walked — and the qualification is what makes that safe
//! The rule this module and `lang::symbols` both encode: **attribute a call only to a symbol
//! `lang::symbols` actually emits.** Two earlier arrangements each broke it in one direction, and both
//! failures were measured through the real engine:
//!
//! 1. Until 2026-08-10 this module walked into `Item::Mod` while `lang::symbols` did not. A `RawCall`
//!    attributed to a nested item had no symbol of its own to name, so it borrowed the id an item at
//!    file top level WOULD have: `mod v1 { fn handler() }` and a top-level `fn handler()` produced the
//!    SAME `from_symbol`, and `build_symbol_graph` buckets by file and emitted both sets of edges from
//!    that one node. A Rust tree whose deployed handler checks nothing was flagged by
//!    `mutating-route-no-auth`; appending a legacy `mod v1` whose homonym handler called `verify_token()`
//!    — a function the DEPLOYED route never reaches — silenced the finding entirely.
//! 2. The narrowing that fixed it (stop walking) closed the false-negative but paid recall: every call
//!    written inside an inline `mod` became invisible, so a guard a handler genuinely reaches only
//!    through an inline-mod helper could no longer clear its route.
//!
//! `lang::symbols` now QUALIFIES a nested item's name with its inline-`mod` chain (`x::inner`), which
//! removes the premise both failures rested on: a nested `handler` and a top-level `handler` are two
//! distinct ids, so walking in cannot forge the first failure, and not walking in is no longer the only
//! way to avoid it. This module walks the same chain and builds the same qualified id.
//! `inline_mod_calls_are_not_attributed_to_a_homonym_top_level_symbol` and
//! `every_from_symbol_is_a_symbol_parse_symbols_emits` in this module's tests pin the pair, so the two
//! files cannot drift back into opposite premises.
//!
//! ## What the CALLEE side qualifies, and what it deliberately leaves bare
//! `from_symbol` is only half the id question — a call's own name has to reach the right node too.
//! [`Level`] answers it with the narrowest rule that cannot invent an edge:
//! - A bare `f()` written inside `mod x` becomes `x::f` **only when `x`'s own item list declares `f`**.
//!   Otherwise it stands as written, so it resolves to the file-level or imported `f` — which is what
//!   Rust name resolution would do.
//! - `x::f()` becomes `x::f` with NO `receiver_type` when `x` is an inline `mod` declared at the same
//!   level. Without this the qualifier would be handed to `resolve_method` as if it were a TYPE, and a
//!   module is not a type.
//! - Everything else is left exactly as written. In particular a METHOD call (`v.f()`) is never
//!   qualified: its receiver is a value, not a module path, and rewriting it would claim a containment
//!   the expression does not state.
//!
//! Two shadowing shapes stay unmodelled, both MISSES rather than wrong edges: a name declared in `x` and
//! called from `x::y` (only the innermost level's own item list is consulted), and a `use` written
//! inside an inline `mod` (`lang::imports` reads file-level `use` only, so such a binding is invisible
//! to resolution regardless of what this module records).

mod expr;

use std::collections::HashSet;

use syn::{ImplItem, Item};

use zzop_core::callgraph::RawCall;

use super::symbols::qualify;

/// Every call site in `text`, attributed to the enclosing symbol. Empty when `syn` cannot parse (the
/// caller has already degraded to lexical in that case).
pub fn parse_calls(rel: &str, text: &str) -> Vec<RawCall> {
    let Ok(file) = syn::parse_file(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk_items(rel, &file.items, &[], &mut out);
    out
}

/// One inline-`mod` level's own naming context — see the module doc's callee-side section. Built once
/// per level (never per call) because it is a property of the item list, not of any expression.
pub(super) struct Level {
    /// The enclosing inline-`mod` chain, outermost first. Empty at file top level.
    path: Vec<String>,
    /// Names this level's own item list declares — the set `lang::symbols` emits here as bare leaves.
    /// `impl` members are absent on purpose: a `Type.method` is not callable by a bare name.
    declared: HashSet<String>,
    /// Inline `mod` names this level's own item list declares.
    inline_mods: HashSet<String>,
}

impl Level {
    fn of(items: &[Item], path: &[String]) -> Self {
        let mut declared = HashSet::new();
        let mut inline_mods = HashSet::new();
        for item in items {
            let ident = match item {
                Item::Fn(f) => Some(f.sig.ident.to_string()),
                Item::Struct(s) => Some(s.ident.to_string()),
                Item::Enum(e) => Some(e.ident.to_string()),
                Item::Union(u) => Some(u.ident.to_string()),
                Item::Trait(t) => Some(t.ident.to_string()),
                Item::Type(t) => Some(t.ident.to_string()),
                Item::Const(c) => Some(c.ident.to_string()),
                Item::Static(s) => Some(s.ident.to_string()),
                Item::Mod(m) if m.content.is_some() => {
                    inline_mods.insert(m.ident.to_string());
                    None
                }
                _ => None,
            };
            if let Some(ident) = ident {
                declared.insert(ident);
            }
        }
        Self {
            path: path.to_vec(),
            declared,
            inline_mods,
        }
    }

    /// A bare `f()` written at this level — module doc's callee-side rule.
    pub(super) fn callee(&self, name: &str) -> String {
        if self.declared.contains(name) {
            qualify(&self.path, name)
        } else {
            name.to_string()
        }
    }

    /// `q::f()` written at this level, when `q` is an inline `mod` declared here — `Some` carries the
    /// qualified callee and means the caller must drop `receiver_type` (a module is not a type).
    pub(super) fn path_callee(&self, qualifier: &str, name: &str) -> Option<String> {
        if !self.inline_mods.contains(qualifier) {
            return None;
        }
        let mut path = self.path.clone();
        path.push(qualifier.to_string());
        Some(qualify(&path, name))
    }
}

/// One walk position: the symbol id calls are attributed to, plus the level that names them.
pub(super) struct Cx<'a> {
    pub(super) from: String,
    pub(super) level: &'a Level,
}

fn walk_items(rel: &str, items: &[Item], path: &[String], out: &mut Vec<RawCall>) {
    let level = Level::of(items, path);
    for item in items {
        match item {
            Item::Fn(f) => {
                let name = qualify(path, &f.sig.ident.to_string());
                let cx = Cx {
                    from: format!("{rel}#{name}"),
                    level: &level,
                };
                expr::walk_block(&cx, &f.block, out);
            }
            Item::Impl(imp) => {
                // The symbol id an impl method gets is `<mod path>::<Type>.<method>` (see
                // `symbols::emit::emit_impl`) — this must agree byte-for-byte or every edge from a
                // method dangles.
                let Some(type_name) = super::symbols::type_leaf_name(&imp.self_ty) else {
                    continue;
                };
                for it in &imp.items {
                    if let ImplItem::Fn(f) = it {
                        let name = qualify(path, &format!("{type_name}.{}", f.sig.ident));
                        let cx = Cx {
                            from: format!("{rel}#{name}"),
                            level: &level,
                        };
                        expr::walk_block(&cx, &f.block, out);
                    }
                }
            }
            // An INLINE `mod x { ... }` is walked with `x` pushed onto the chain — module doc. A
            // `mod x;` DECLARATION (`content: None`) names another FILE and has no body here.
            Item::Mod(m) => {
                if let Some((_brace, inner)) = &m.content {
                    let mut nested = path.to_vec();
                    nested.push(m.ident.to_string());
                    walk_items(rel, inner, &nested, out);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
