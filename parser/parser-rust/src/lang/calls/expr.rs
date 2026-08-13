//! The EXPRESSION walk — split out from `calls.rs` purely to keep that file under the line-count
//! ratchet. The parent module owns the scope contract (which items are walked, and how a call's own name
//! is qualified); this file owns the descent and the two `RawCall` push sites.

use syn::spanned::Spanned;
use syn::{Expr, Stmt};

use zzop_core::callgraph::RawCall;

use super::Cx;

pub(super) fn walk_block(cx: &Cx, block: &syn::Block, out: &mut Vec<RawCall>) {
    for stmt in &block.stmts {
        walk_stmt(cx, stmt, out);
    }
}

fn walk_stmt(cx: &Cx, stmt: &Stmt, out: &mut Vec<RawCall>) {
    match stmt {
        Stmt::Local(local) => {
            if let Some(init) = &local.init {
                walk_expr(cx, &init.expr, out);
                if let Some((_, diverge)) = &init.diverge {
                    walk_expr(cx, diverge, out);
                }
            }
        }
        Stmt::Expr(e, _) => walk_expr(cx, e, out),
        // A `fn`/`impl` declared INSIDE a body gets no `SourceSymbol` of its own, so — by the parent
        // module doc's rule — there is no id its calls could honestly be attributed to. Unlike an inline
        // `mod`, a body-local item has no nameable path for `lang::symbols` to qualify with, so this
        // stays skipped.
        Stmt::Item(_) | Stmt::Macro(_) => {}
    }
}

/// Records `expr` if it is a call, then descends into every sub-expression.
///
/// Written as one exhaustive-ish walk rather than `syn::visit` to keep the crate's dependency surface as
/// it is (this crate takes `syn` with the features it already needs); the cost is that a shape not listed
/// below is not descended into. That is a MISS, never a wrong edge — the direction this repo's
/// extraction discipline requires.
fn walk_expr(cx: &Cx, expr: &Expr, out: &mut Vec<RawCall>) {
    match expr {
        Expr::Call(c) => {
            if let Expr::Path(p) = &*c.func {
                let mut segs = p.path.segments.iter().rev();
                if let Some(last) = segs.next() {
                    let leaf = last.ident.to_string();
                    let qualifier = segs.next().map(|s| s.ident.to_string());
                    // Parent module doc's callee-side rules: an inline-`mod` qualifier resolves to a
                    // qualified NAME (a module is not a type, so `receiver_type` must be dropped);
                    // anything else keeps the `Type::assoc` reading it has always had.
                    let (callee_name, receiver_type) = match qualifier
                        .as_deref()
                        .and_then(|q| cx.level.path_callee(q, &leaf))
                    {
                        Some(qualified) => (qualified, None),
                        None if qualifier.is_some() => (leaf, qualifier),
                        None => (cx.level.callee(&leaf), None),
                    };
                    out.push(RawCall {
                        from_symbol: cx.from.clone(),
                        callee_name,
                        line: c.span().start().line as u32,
                        receiver_type,
                        is_heritage: false,
                    });
                }
            }
            walk_expr(cx, &c.func, out);
            for a in &c.args {
                walk_expr(cx, a, out);
            }
        }
        Expr::MethodCall(m) => {
            out.push(RawCall {
                from_symbol: cx.from.clone(),
                // Never inline-`mod`-qualified — the receiver is a VALUE, not a module path (parent
                // module doc's "left bare" clause).
                callee_name: m.method.to_string(),
                line: m.span().start().line as u32,
                // No type layer, so no receiver type — see the parent module doc's trait-dispatch note.
                receiver_type: None,
                is_heritage: false,
            });
            walk_expr(cx, &m.receiver, out);
            for a in &m.args {
                walk_expr(cx, a, out);
            }
        }
        Expr::Await(a) => walk_expr(cx, &a.base, out),
        Expr::Try(t) => walk_expr(cx, &t.expr, out),
        Expr::Paren(p) => walk_expr(cx, &p.expr, out),
        Expr::Group(g) => walk_expr(cx, &g.expr, out),
        Expr::Reference(r) => walk_expr(cx, &r.expr, out),
        Expr::Unary(u) => walk_expr(cx, &u.expr, out),
        Expr::Cast(c) => walk_expr(cx, &c.expr, out),
        Expr::Field(f) => walk_expr(cx, &f.base, out),
        Expr::Binary(b) => {
            walk_expr(cx, &b.left, out);
            walk_expr(cx, &b.right, out);
        }
        Expr::Assign(a) => {
            walk_expr(cx, &a.left, out);
            walk_expr(cx, &a.right, out);
        }
        Expr::Index(i) => {
            walk_expr(cx, &i.expr, out);
            walk_expr(cx, &i.index, out);
        }
        Expr::Let(l) => walk_expr(cx, &l.expr, out),
        Expr::Return(r) => {
            if let Some(e) = &r.expr {
                walk_expr(cx, e, out);
            }
        }
        Expr::Break(b) => {
            if let Some(e) = &b.expr {
                walk_expr(cx, e, out);
            }
        }
        Expr::Closure(c) => walk_expr(cx, &c.body, out),
        Expr::Async(a) => walk_block(cx, &a.block, out),
        Expr::Unsafe(u) => walk_block(cx, &u.block, out),
        Expr::Block(b) => walk_block(cx, &b.block, out),
        Expr::Loop(l) => walk_block(cx, &l.body, out),
        Expr::While(w) => {
            walk_expr(cx, &w.cond, out);
            walk_block(cx, &w.body, out);
        }
        Expr::ForLoop(f) => {
            walk_expr(cx, &f.expr, out);
            walk_block(cx, &f.body, out);
        }
        Expr::If(i) => {
            walk_expr(cx, &i.cond, out);
            walk_block(cx, &i.then_branch, out);
            if let Some((_, e)) = &i.else_branch {
                walk_expr(cx, e, out);
            }
        }
        Expr::Match(m) => {
            walk_expr(cx, &m.expr, out);
            for arm in &m.arms {
                if let Some((_, g)) = &arm.guard {
                    walk_expr(cx, g, out);
                }
                walk_expr(cx, &arm.body, out);
            }
        }
        Expr::Array(a) => {
            for e in &a.elems {
                walk_expr(cx, e, out);
            }
        }
        Expr::Tuple(t) => {
            for e in &t.elems {
                walk_expr(cx, e, out);
            }
        }
        Expr::Struct(s) => {
            for f in &s.fields {
                walk_expr(cx, &f.expr, out);
            }
        }
        _ => {}
    }
}
