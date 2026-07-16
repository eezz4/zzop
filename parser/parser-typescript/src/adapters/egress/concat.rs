//! `+` string-concatenation URL resolution (`str-concat-url-v1`) — the isomorphic counterpart to
//! template-literal resolution; see the module doc on [`super`] for the full rule.

use std::collections::HashMap;

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{BinaryOp, Expr, Lit};

use super::unwrap_expr;
use super::url_resolve::{dedup_preserve_order, expr_text, TplPiece};

/// `+` string-concatenation -> URL variants (`str-concat-url-v1`), the isomorphic counterpart to
/// [`resolve_template_variants`](super::url_resolve) for binary `+` instead of a template literal. Flattens the
/// left-associative chain, rejects it (empty vec) if any operator isn't `+` or if no operand is a direct
/// string literal, then maps each operand to a [`TplPiece`] and assembles variants with
/// [`assemble_concat_variants`]. See the module doc for the full rule.
pub(super) fn resolve_concat_variants(
    arg: &Expr,
    consts: &HashMap<String, String>,
    cm: &SourceMap,
) -> Vec<String> {
    let Some(operands) = flatten_add_chain(arg) else {
        return Vec::new();
    };
    let has_literal_operand = operands
        .iter()
        .any(|o| matches!(unwrap_expr(o), Expr::Lit(Lit::Str(_))));
    if !has_literal_operand {
        return Vec::new();
    }
    let pieces: Vec<TplPiece> = operands
        .iter()
        .map(|o| concat_operand_piece(o, consts, cm))
        .collect();
    assemble_concat_variants(&pieces)
}

/// Flatten a left-associative `+` chain (`a + b + c` parses as `Bin{Bin{a,b},c}`) into its ordered
/// operands, or `None` if any operator in the chain is not `+` — never guessed for `-`, `??`, `||`, etc.
/// A wrapper (`(...)`, `as const`, ...) around the chain or one of its sub-chains is stripped via
/// [`unwrap_expr`] before matching.
fn flatten_add_chain(expr: &Expr) -> Option<Vec<&Expr>> {
    match unwrap_expr(expr) {
        Expr::Bin(b) => {
            if b.op != BinaryOp::Add {
                return None;
            }
            let mut operands = flatten_add_chain(&b.left)?;
            operands.push(&b.right);
            Some(operands)
        }
        other => Some(vec![other]),
    }
}

/// Map one `+`-chain operand to a [`TplPiece`]: a string literal is `Fixed`; an identifier/member that
/// resolves in `consts` (same lookup as `resolve_url_variants`'s own `Expr::Ident|Member` arm) is
/// `Fixed`; a ternary with BOTH arms string literals is the fan-out `Slot`; anything else (an unresolved
/// identifier, a call, a nested non-string expression) is the old `Fixed("{}")` placeholder.
fn concat_operand_piece(
    operand: &Expr,
    consts: &HashMap<String, String>,
    cm: &SourceMap,
) -> TplPiece {
    match unwrap_expr(operand) {
        Expr::Lit(Lit::Str(s)) => TplPiece::Fixed(s.value.as_str().unwrap_or_default().to_string()),
        e @ (Expr::Ident(_) | Expr::Member(_)) => match consts.get(&expr_text(e, cm)) {
            Some(v) => TplPiece::Fixed(v.clone()),
            None => TplPiece::Fixed("{}".to_string()),
        },
        Expr::Cond(c) => {
            if let (Expr::Lit(Lit::Str(cons)), Expr::Lit(Lit::Str(alt))) = (&*c.cons, &*c.alt) {
                TplPiece::Slot(
                    cons.value.as_str().unwrap_or_default().to_string(),
                    alt.value.as_str().unwrap_or_default().to_string(),
                )
            } else {
                TplPiece::Fixed("{}".to_string())
            }
        }
        _ => TplPiece::Fixed("{}".to_string()),
    }
}

/// Assemble a `+`-chain's pieces into URL variants: concatenate `Fixed` pieces inline, cartesian-product
/// `Slot` pieces, capped at 2 slots (a 3rd+ slot forces every slot in THIS chain back to fixed `"{}"`,
/// same bounded-output rule as [`resolve_template_variants`](super::url_resolve)), then dedup preserving first-seen order.
/// Standalone from `resolve_template_variants`'s assembly loop — deliberately NOT shared, so that loop's
/// existing tests stay byte-identical (see module doc / task notes): a concat chain has no quasis, so
/// pieces are just concatenated in sequence rather than interleaved with quasi text.
fn assemble_concat_variants(pieces: &[TplPiece]) -> Vec<String> {
    let slot_count = pieces
        .iter()
        .filter(|p| matches!(p, TplPiece::Slot(_, _)))
        .count();
    let mut variants = vec![String::new()];
    for p in pieces {
        match p {
            TplPiece::Fixed(s) => {
                for v in variants.iter_mut() {
                    v.push_str(s);
                }
            }
            TplPiece::Slot(cons, alt) => {
                if slot_count > 2 {
                    for v in variants.iter_mut() {
                        v.push_str("{}");
                    }
                } else {
                    let mut next = Vec::with_capacity(variants.len() * 2);
                    for v in &variants {
                        let mut a = v.clone();
                        a.push_str(cons);
                        next.push(a);
                        let mut b = v.clone();
                        b.push_str(alt);
                        next.push(b);
                    }
                    variants = next;
                }
            }
        }
    }
    dedup_preserve_order(variants)
}

#[cfg(test)]
mod tests {
    use crate::adapters::egress::{extract_http_egress, files, keys};

    // --- `+` string-concatenation URLs (`str-concat-url-v1`) ---

    #[test]
    fn str_concat_literal_plus_variable_keys_as_param() {
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get('/profiles/' + username)")]));
        assert_eq!(out[0].key.as_deref(), Some("GET /profiles/{}"));
    }

    #[test]
    fn str_concat_three_way_with_trailing_literal() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.post('/profiles/' + username + '/follow', body)",
        )]));
        assert_eq!(out[0].key.as_deref(), Some("POST /profiles/{}/follow"));
    }

    #[test]
    fn str_concat_with_conditional_literal_fans_out() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.get('/articles' + (feed ? '/feed' : ''))",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("GET /articles/feed".to_string()),
                Some("GET /articles".to_string()),
            ]
        );
    }

    #[test]
    fn str_concat_with_no_string_literal_is_unresolved() {
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get(base + path)")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("base + path"));
    }

    #[test]
    fn str_concat_non_plus_operator_is_unresolved() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.get('/a' - x); axios.get('/b' ?? y);",
        )]));
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|c| c.key.is_none()));
    }

    #[test]
    fn str_concat_opaque_prefix_now_keys_via_head_drop() {
        // A bare `const BASE = '/api'; axios.get(BASE + '/users')` still never resolves the VALUE of
        // BASE: a bare undotted const is not captured by `const_map_fragment` (project-wide
        // scope-insensitive lookup would let a common name shadow a function param and mis-key —
        // never-guess). BASE -> `{}`, so the concat assembles to `"{}/users"`. That used to dead-end at
        // `base_relative_path`'s `{`-veto; now `consume_key_for`'s base-carrier head-drop bucket catches
        // this exact shape (single leading `{}` + `/`-headed literal) and drops the opaque base rather
        // than valuing it, keying on the visible `/users` literal. (str-concat-url-v1 resolves the visible
        // LITERAL operands, not const-prefix indirection — cross-layer-resolution.md; the shadow-risk
        // revert on BASE's value still stands.)
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "const BASE = '/api'; axios.get(BASE + '/users')",
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /users"));
    }
}
