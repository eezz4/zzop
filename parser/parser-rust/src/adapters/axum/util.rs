//! Small pure `syn`-expression helpers for the axum adapter — extracted from `axum.rs` (file-size
//! limit). No adapter state, just shape predicates over `Expr`/`Pat`.

use syn::{Expr, Lit, Pat};

/// `Router::new()` / `axum::Router::new()` — a `<...>::Router::new()` call whose last two path segments
/// are `Router::new`.
pub(super) fn is_router_new_call(expr: &Expr) -> bool {
    let Expr::Call(call) = expr else { return false };
    let Expr::Path(p) = &*call.func else {
        return false;
    };
    let segs = &p.path.segments;
    let n = segs.len();
    n >= 2 && segs[n - 1].ident == "new" && segs[n - 2].ident == "Router"
}

/// True when `expr` is exactly the bare identifier `name`.
pub(super) fn is_same_ident(expr: &Expr, name: &str) -> bool {
    simple_expr_ident(expr).as_deref() == Some(name)
}

/// The single-segment identifier `expr` names, or `None` for any other (multi-segment / non-path) shape.
pub(super) fn simple_expr_ident(expr: &Expr) -> Option<String> {
    let Expr::Path(p) = expr else { return None };
    if p.path.segments.len() != 1 {
        return None;
    }
    Some(p.path.segments[0].ident.to_string())
}

/// The identifier a simple `let <name>` pattern binds, or `None` for a destructuring/typed pattern.
pub(super) fn simple_pat_ident(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(pi) => Some(pi.ident.to_string()),
        _ => None,
    }
}

/// The value of a string-literal expression, or `None` for any other expression.
pub(super) fn string_literal(expr: &Expr) -> Option<String> {
    let Expr::Lit(el) = expr else { return None };
    let Lit::Str(s) = &el.lit else { return None };
    Some(s.value())
}
