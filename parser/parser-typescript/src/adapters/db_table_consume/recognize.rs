//! The shared `<base>.<accessor>.<method>(...)` member-chain matcher for `db_table_consume` — both
//! anchor forms (`getPrisma()` zero-arg call, bare Prisma-bound receiver identifier) go through
//! `match_prisma_model_call`. See the parent module doc for the full anchor-form spec.

use std::collections::HashSet;

use swc_core::common::Span;
use swc_core::ecma::ast::{CallExpr, Callee, Expr, MemberProp};

use super::PRISMA_CLIENT_GETTER;

/// One `<base>.<accessor>.<method>` match, shared by both collectors in the parent module — `<base>`
/// may be either anchor form `match_prisma_model_call` recognizes.
pub(super) struct PrismaModelCall {
    /// The plain-identifier model accessor (`item` in `getPrisma().item.findMany(...)`).
    pub(super) accessor: String,
    /// The method identifier (`findMany`, `create`, ...) — unfiltered; callers restrict as needed.
    pub(super) method: String,
    /// The `<base>.<accessor>.<method>` member expression's own span — `.hi` sits immediately before
    /// the call's own `(`, so `extract_query_call_sites` can slice the argument-list text from it.
    pub(super) callee_span: Span,
}

/// The `<base>.<accessor>.<method>` member-chain shape, with `<base>` left unvalidated for the caller to
/// judge (the two different anchor forms `match_prisma_model_call` accepts).
struct MemberChain<'e> {
    base: &'e Expr,
    accessor: String,
    method: String,
    callee_span: Span,
}

/// Matches the `<base>.<accessor>.<method>(...)` shape, returning `<base>` (unwrapped) plus the
/// accessor/method identifiers and the callee member expression's span. `None` on any gate failure: a
/// non-member callee or a computed segment anywhere in the chain.
fn match_member_chain(call: &CallExpr) -> Option<MemberChain<'_>> {
    // `<obj>.<method>(...)`
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(outer) = unwrap_expr(callee) else {
        return None;
    };
    let MemberProp::Ident(method) = &outer.prop else {
        return None; // computed `[...]` method — dynamic, skip honestly
    };
    // `<base>.<accessor>`
    let Expr::Member(mid) = unwrap_expr(&outer.obj) else {
        return None;
    };
    let MemberProp::Ident(accessor) = &mid.prop else {
        return None;
    };
    Some(MemberChain {
        base: unwrap_expr(&mid.obj),
        accessor: accessor.sym.to_string(),
        method: method.sym.to_string(),
        callee_span: outer.span,
    })
}

/// Matches `<getter>().<accessor>.<method>(...)` (a zero-arg call to [`PRISMA_CLIENT_GETTER`]) OR
/// `<receiver>.<accessor>.<method>(...)` where `<receiver>` is a plain identifier in `receivers` — see
/// the parent module doc for both anchor forms, and `super::receivers::prisma_bound_receivers` for how
/// the second form's evidence is gathered.
pub(super) fn match_prisma_model_call(
    call: &CallExpr,
    receivers: &HashSet<String>,
) -> Option<PrismaModelCall> {
    let chain = match_member_chain(call)?;
    let anchored = match chain.base {
        Expr::Call(base) => {
            base.args.is_empty()
                && matches!(&base.callee, Callee::Expr(c) if matches!(unwrap_expr(c), Expr::Ident(id) if id.sym == PRISMA_CLIENT_GETTER))
        }
        Expr::Ident(id) => receivers.contains(id.sym.as_str()),
        _ => false,
    };
    if !anchored {
        return None;
    }
    Some(PrismaModelCall {
        accessor: chain.accessor,
        method: chain.method,
        callee_span: chain.callee_span,
    })
}

/// Unwraps parens/`as`/`!`/`satisfies` wrappers to the inner expression. Local copy, mirroring the other
/// adapters in this crate. `pub(super)` so `receivers.rs` (a sibling module) can share it.
pub(super) fn unwrap_expr(e: &Expr) -> &Expr {
    match e {
        Expr::Paren(p) => unwrap_expr(&p.expr),
        Expr::TsAs(a) => unwrap_expr(&a.expr),
        Expr::TsNonNull(n) => unwrap_expr(&n.expr),
        Expr::TsSatisfies(s) => unwrap_expr(&s.expr),
        other => other,
    }
}

/// First-char-uppercase (`item` -> `Item`) — mirrors `zzop_rules_schema::usage::capitalize` byte-for-byte;
/// duplicated locally to avoid a parser-typescript -> rules-schema dependency edge for one function.
pub(super) fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
