//! Correlated-ternary fan-out pairing (`correlated-ternary-pair-v1`): when a call's verb fan-out and URL
//! fan-out are governed by the SAME ternary guard, the reachable combinations are the index-paired arms,
//! not the full Cartesian product. Used by [`super::collector`]'s emit loop.

use swc_core::common::{SourceMap, SourceMapper, Spanned};
use swc_core::ecma::ast::{CallExpr, Callee, Expr, MemberProp};

use super::matchers::HttpCall;
use super::unwrap_expr;

/// The (method, url) combinations to emit for a resolved call. Normally the full Cartesian product of
/// `hc.methods` × `url_variants`, EXCEPT the correlated-ternary case: when the verb fan-out
/// (`axios[G ? 'put' : 'post']`) and the URL fan-out (`` `/articles${G ? `/${slug}` : ''}` ``) are both
/// 2-way ternaries governed by the SAME guard expression `G`, only the two reachable branches exist
/// (`(put, /articles/{})` and `(post, /articles)`), NOT the two cross combos. Both fan-outs are produced
/// cons-arm-first, so index-pairing (cons↔cons, alt↔alt) is the correct correlation. Guards are compared
/// by source text; any mismatch, a non-ternary shape, or >1 URL ternary falls back to the Cartesian
/// product (the safe over-approximation for genuinely-independent conditions).
pub(super) fn method_url_pairs<'a>(
    call: &CallExpr,
    hc: &'a HttpCall,
    url_variants: &'a [String],
    cm: &SourceMap,
) -> Vec<(&'a String, &'a String)> {
    if hc.methods.len() == 2 && url_variants.len() == 2 {
        if let (Some(vg), Some(ug)) = (verb_ternary_guard(call, cm), url_ternary_guard(hc.arg, cm))
        {
            if vg == ug {
                return vec![
                    (&hc.methods[0], &url_variants[0]),
                    (&hc.methods[1], &url_variants[1]),
                ];
            }
        }
    }
    hc.methods
        .iter()
        .flat_map(|m| url_variants.iter().map(move |u| (m, u)))
        .collect()
}

/// Source text of the ternary guard governing a computed-member verb fan-out (`axios[G ? 'a' : 'b']`), or
/// `None` when the callee is not a computed-member ternary.
fn verb_ternary_guard(call: &CallExpr, cm: &SourceMap) -> Option<String> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(m) = &**callee else {
        return None;
    };
    let MemberProp::Computed(c) = &m.prop else {
        return None;
    };
    let Expr::Cond(cond) = &*c.expr else {
        return None;
    };
    Some(
        cm.span_to_snippet(cond.test.span())
            .ok()?
            .trim()
            .to_string(),
    )
}

/// Source text of the ternary guard governing a URL fan-out — a top-level `G ? '/a' : '/b'` argument, or a
/// template with EXACTLY ONE ternary interpolation `` `/x${G ? a : b}` ``. `None` when the argument carries
/// no ternary, or more than one (ambiguous — correlation would be a guess, so fall back to Cartesian).
fn url_ternary_guard(arg: &Expr, cm: &SourceMap) -> Option<String> {
    match unwrap_expr(arg) {
        Expr::Cond(c) => Some(cm.span_to_snippet(c.test.span()).ok()?.trim().to_string()),
        Expr::Tpl(t) => {
            let mut guard = None;
            for e in &t.exprs {
                if let Expr::Cond(c) = &**e {
                    if guard.is_some() {
                        return None; // >1 ternary — ambiguous
                    }
                    guard = Some(cm.span_to_snippet(c.test.span()).ok()?.trim().to_string());
                }
            }
            guard
        }
        _ => None,
    }
}
