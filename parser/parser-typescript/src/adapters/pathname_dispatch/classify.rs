// Test/conjunct classification

use swc_core::common::BytePos;
use swc_core::ecma::ast::{BinaryOp, Expr, Lit};
use zzop_core::HTTP_KEY_VERBS;

use super::ctx::{is_method_receiver, is_pathname_receiver, pathname_match_route, FnCtx};
use super::push_unique;

fn unwrap_parens(mut expr: &Expr) -> &Expr {
    while let Expr::Paren(p) = expr {
        expr = &p.expr;
    }
    expr
}

pub(super) fn split_and(expr: &Expr) -> Vec<&Expr> {
    let e = unwrap_parens(expr);
    if let Expr::Bin(b) = e {
        if b.op == BinaryOp::LogicalAnd {
            let mut out = split_and(&b.left);
            out.extend(split_and(&b.right));
            return out;
        }
    }
    vec![e]
}

fn split_or(expr: &Expr) -> Vec<&Expr> {
    let e = unwrap_parens(expr);
    if let Expr::Bin(b) = e {
        if b.op == BinaryOp::LogicalOr {
            let mut out = split_or(&b.left);
            out.extend(split_or(&b.right));
            return out;
        }
    }
    vec![e]
}

/// A path-test literal: a plain string literal, or a zero-interpolation template literal
/// (cooked) — see module doc's "Path test".
pub(super) fn path_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        Expr::Tpl(t) if t.exprs.is_empty() && t.quasis.len() == 1 => t.quasis[0]
            .cooked
            .as_ref()
            .and_then(|c| c.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// A verb-test literal: a plain string literal only (no template-literal vocabulary for verbs).
pub(super) fn verb_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}

/// A single path test: `===`/`==` only (never `!==`/`!=` — module doc), either operand order,
/// between a pathname-provenanced receiver and a `/`-leading literal. Returns the literal path
/// and the comparison's own span start (used for `IoProvide::line`).
fn path_test(expr: &Expr, ctx: &FnCtx) -> Option<(String, BytePos)> {
    let Expr::Bin(b) = unwrap_parens(expr) else {
        return None;
    };
    if !matches!(b.op, BinaryOp::EqEq | BinaryOp::EqEqEq) {
        return None;
    }
    let lit = if is_pathname_receiver(&b.left, ctx) {
        &b.right
    } else if is_pathname_receiver(&b.right, ctx) {
        &b.left
    } else {
        return None;
    };
    let path = path_literal(lit)?;
    if !path.starts_with('/') {
        return None;
    }
    Some((path, b.span.lo))
}

/// A `pathname.match(/re/)` path test: either a bare reference to a `const m = pathname.match(...)`
/// binding (recorded in `ctx.pathname_match_routes` by the binding collector) or an inline
/// `pathname.match(/re/)` call, in each case optionally compared `!== null`/`!= null`. Returns the
/// converted route path and a span (the regex literal's, for `IoProvide::line`).
fn match_path_test(expr: &Expr, ctx: &FnCtx) -> Option<(String, BytePos)> {
    let e = strip_null_compare(unwrap_parens(expr));
    match e {
        // `if (verifyMatch && ...)` — a reference to the recorded match binding.
        Expr::Ident(id) => ctx
            .pathname_match_routes
            .get(id.sym.as_str())
            .map(|(path, pos)| (path.clone(), *pos)),
        // `if (pathname.match(/re/) && ...)` — the match inline in the test (same recognizer the
        // binding collector uses, applied with this function's own provenance sets).
        Expr::Call(_) => pathname_match_route(e, &ctx.pathname_aliases, &ctx.url_provenanced),
        _ => None,
    }
}

/// Either a literal `pathname === "..."` path test or a `pathname.match(/re/)` one.
fn path_test_any(expr: &Expr, ctx: &FnCtx) -> Option<(String, BytePos)> {
    path_test(expr, ctx).or_else(|| match_path_test(expr, ctx))
}

/// Unwraps a `<x> !== null`/`<x> != null`/`<x> !== undefined` truthiness guard to `<x>` (either
/// operand order); returns `expr` unchanged when it is not such a null comparison.
fn strip_null_compare(expr: &Expr) -> &Expr {
    let Expr::Bin(b) = expr else { return expr };
    if !matches!(b.op, BinaryOp::NotEq | BinaryOp::NotEqEq) {
        return expr;
    }
    let (l, r) = (unwrap_parens(&b.left), unwrap_parens(&b.right));
    if is_null_or_undefined(r) {
        l
    } else if is_null_or_undefined(l) {
        r
    } else {
        expr
    }
}

fn is_null_or_undefined(e: &Expr) -> bool {
    matches!(e, Expr::Lit(Lit::Null(_))) || matches!(e, Expr::Ident(id) if id.sym == "undefined")
}

/// A single verb mention: `===`/`==`/`!==`/`!=` (mentioned-verb semantics — module doc), either
/// operand order, between a method-provenanced receiver and an `HTTP_KEY_VERBS` literal.
fn verb_test(expr: &Expr, ctx: &FnCtx) -> Option<String> {
    let Expr::Bin(b) = unwrap_parens(expr) else {
        return None;
    };
    if !matches!(
        b.op,
        BinaryOp::EqEq | BinaryOp::EqEqEq | BinaryOp::NotEq | BinaryOp::NotEqEq
    ) {
        return None;
    }
    let lit = if is_method_receiver(&b.left, ctx) {
        &b.right
    } else if is_method_receiver(&b.right, ctx) {
        &b.left
    } else {
        return None;
    };
    let verb = verb_literal(lit)?;
    if HTTP_KEY_VERBS.contains(&verb.as_str()) {
        Some(verb)
    } else {
        None
    }
}

/// What one `&&`-conjunct of an `if`'s test contributes.
pub(super) enum Conjunct {
    Paths(Vec<(String, BytePos)>),
    Verbs(Vec<String>),
    /// Not a path test, verb test, or all-same-shape `||` of either — contributes nothing
    /// (covers a mixed `(path || flag)` disjunction — module doc: never guessed).
    Other,
}

pub(super) fn classify_conjunct(expr: &Expr, ctx: &FnCtx) -> Conjunct {
    let e = unwrap_parens(expr);
    if let Expr::Bin(b) = e {
        if b.op == BinaryOp::LogicalOr {
            return classify_or_disjuncts(&split_or(e), ctx);
        }
    }
    if let Some(p) = path_test_any(e, ctx) {
        return Conjunct::Paths(vec![p]);
    }
    if let Some(v) = verb_test(e, ctx) {
        return Conjunct::Verbs(vec![v]);
    }
    Conjunct::Other
}

fn classify_or_disjuncts(disjuncts: &[&Expr], ctx: &FnCtx) -> Conjunct {
    let mut paths = Vec::new();
    let mut all_path = true;
    for d in disjuncts {
        match path_test_any(d, ctx) {
            Some(p) => paths.push(p),
            None => {
                all_path = false;
                break;
            }
        }
    }
    if all_path {
        return Conjunct::Paths(paths);
    }
    let mut verbs = Vec::new();
    let mut all_verb = true;
    for d in disjuncts {
        match verb_test(d, ctx) {
            Some(v) => push_unique(&mut verbs, v),
            None => {
                all_verb = false;
                break;
            }
        }
    }
    if all_verb {
        return Conjunct::Verbs(verbs);
    }
    Conjunct::Other
}
