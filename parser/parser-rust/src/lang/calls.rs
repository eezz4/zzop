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

use syn::spanned::Spanned;
use syn::{Expr, ImplItem, Item, Stmt};

use zzop_core::callgraph::RawCall;

/// Every call site in `text`, attributed to the enclosing top-level symbol. Empty when `syn` cannot
/// parse (the caller has already degraded to lexical in that case).
pub fn parse_calls(rel: &str, text: &str) -> Vec<RawCall> {
    let Ok(file) = syn::parse_file(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in &file.items {
        walk_item(rel, item, &mut out);
    }
    out
}

fn walk_item(rel: &str, item: &Item, out: &mut Vec<RawCall>) {
    match item {
        Item::Fn(f) => {
            let from = format!("{rel}#{}", f.sig.ident);
            walk_block(&from, &f.block, out);
        }
        Item::Impl(imp) => {
            // The symbol id an impl method gets is `<Type>.<method>` (see `symbols::emit_impl`) — this
            // must agree byte-for-byte or every edge from a method dangles.
            let Some(type_name) = super::symbols::type_leaf_name(&imp.self_ty) else {
                return;
            };
            for it in &imp.items {
                if let ImplItem::Fn(f) = it {
                    let from = format!("{rel}#{type_name}.{}", f.sig.ident);
                    walk_block(&from, &f.block, out);
                }
            }
        }
        // A `mod x { ... }` block's items are still THIS file's symbols (`symbols.rs` is file-scoped and
        // v1-flat), so their calls belong to this file too.
        Item::Mod(m) => {
            if let Some((_, items)) = &m.content {
                for inner in items {
                    walk_item(rel, inner, out);
                }
            }
        }
        _ => {}
    }
}

fn walk_block(from: &str, block: &syn::Block, out: &mut Vec<RawCall>) {
    for stmt in &block.stmts {
        walk_stmt(from, stmt, out);
    }
}

fn walk_stmt(from: &str, stmt: &Stmt, out: &mut Vec<RawCall>) {
    match stmt {
        Stmt::Local(local) => {
            if let Some(init) = &local.init {
                walk_expr(from, &init.expr, out);
                if let Some((_, diverge)) = &init.diverge {
                    walk_expr(from, diverge, out);
                }
            }
        }
        Stmt::Expr(e, _) => walk_expr(from, e, out),
        // A nested `fn`/`impl` inside a body is still a top-level-ish symbol elsewhere; its calls are
        // attributed there, not here, so nothing to do.
        Stmt::Item(_) | Stmt::Macro(_) => {}
    }
}

/// Records `expr` if it is a call, then descends into every sub-expression.
///
/// Written as one exhaustive-ish walk rather than `syn::visit` to keep the crate's dependency surface as
/// it is (this crate takes `syn` with the features it already needs); the cost is that a shape not listed
/// below is not descended into. That is a MISS, never a wrong edge — the direction this repo's
/// extraction discipline requires.
fn walk_expr(from: &str, expr: &Expr, out: &mut Vec<RawCall>) {
    match expr {
        Expr::Call(c) => {
            if let Expr::Path(p) = &*c.func {
                let mut segs = p.path.segments.iter().rev();
                if let Some(last) = segs.next() {
                    let receiver_type = segs.next().map(|s| s.ident.to_string());
                    out.push(RawCall {
                        from_symbol: from.to_string(),
                        callee_name: last.ident.to_string(),
                        line: c.span().start().line as u32,
                        receiver_type,
                        is_heritage: false,
                    });
                }
            }
            walk_expr(from, &c.func, out);
            for a in &c.args {
                walk_expr(from, a, out);
            }
        }
        Expr::MethodCall(m) => {
            out.push(RawCall {
                from_symbol: from.to_string(),
                callee_name: m.method.to_string(),
                line: m.span().start().line as u32,
                // No type layer, so no receiver type — see the module doc's trait-dispatch note.
                receiver_type: None,
                is_heritage: false,
            });
            walk_expr(from, &m.receiver, out);
            for a in &m.args {
                walk_expr(from, a, out);
            }
        }
        Expr::Await(a) => walk_expr(from, &a.base, out),
        Expr::Try(t) => walk_expr(from, &t.expr, out),
        Expr::Paren(p) => walk_expr(from, &p.expr, out),
        Expr::Group(g) => walk_expr(from, &g.expr, out),
        Expr::Reference(r) => walk_expr(from, &r.expr, out),
        Expr::Unary(u) => walk_expr(from, &u.expr, out),
        Expr::Cast(c) => walk_expr(from, &c.expr, out),
        Expr::Field(f) => walk_expr(from, &f.base, out),
        Expr::Binary(b) => {
            walk_expr(from, &b.left, out);
            walk_expr(from, &b.right, out);
        }
        Expr::Assign(a) => {
            walk_expr(from, &a.left, out);
            walk_expr(from, &a.right, out);
        }
        Expr::Index(i) => {
            walk_expr(from, &i.expr, out);
            walk_expr(from, &i.index, out);
        }
        Expr::Let(l) => walk_expr(from, &l.expr, out),
        Expr::Return(r) => {
            if let Some(e) = &r.expr {
                walk_expr(from, e, out);
            }
        }
        Expr::Break(b) => {
            if let Some(e) = &b.expr {
                walk_expr(from, e, out);
            }
        }
        Expr::Closure(c) => walk_expr(from, &c.body, out),
        Expr::Async(a) => walk_block(from, &a.block, out),
        Expr::Unsafe(u) => walk_block(from, &u.block, out),
        Expr::Block(b) => walk_block(from, &b.block, out),
        Expr::Loop(l) => walk_block(from, &l.body, out),
        Expr::While(w) => {
            walk_expr(from, &w.cond, out);
            walk_block(from, &w.body, out);
        }
        Expr::ForLoop(f) => {
            walk_expr(from, &f.expr, out);
            walk_block(from, &f.body, out);
        }
        Expr::If(i) => {
            walk_expr(from, &i.cond, out);
            walk_block(from, &i.then_branch, out);
            if let Some((_, e)) = &i.else_branch {
                walk_expr(from, e, out);
            }
        }
        Expr::Match(m) => {
            walk_expr(from, &m.expr, out);
            for arm in &m.arms {
                if let Some((_, g)) = &arm.guard {
                    walk_expr(from, g, out);
                }
                walk_expr(from, &arm.body, out);
            }
        }
        Expr::Array(a) => {
            for e in &a.elems {
                walk_expr(from, e, out);
            }
        }
        Expr::Tuple(t) => {
            for e in &t.elems {
                walk_expr(from, e, out);
            }
        }
        Expr::Struct(s) => {
            for f in &s.fields {
                walk_expr(from, &f.expr, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
