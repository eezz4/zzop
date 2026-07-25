//! `+` string-concatenation URL resolution (`str-concat-url-v1`) — the isomorphic counterpart to
//! template-literal resolution; see the module doc on [`super`] for the full rule.

use std::collections::HashMap;

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{BinaryOp, Expr, Lit};

use super::local_consts::LocalConsts;
use super::unwrap_expr;
use super::url_resolve::{dedup_preserve_order, expr_text, resolve_cond_arm, TplPiece};

/// `+` string-concatenation -> URL variants (`str-concat-url-v1`), the isomorphic counterpart to
/// [`resolve_template_variants`](super::url_resolve) for binary `+` instead of a template literal. Flattens the
/// left-associative chain, rejects it (empty vec) if any operator isn't `+` or if no operand is a direct
/// string literal, then maps each operand to a [`TplPiece`] and assembles variants with
/// [`assemble_concat_variants`]. See the module doc for the full rule.
///
/// The FIRST operand additionally consults `locals` (`same-file-const-prepend-v1`): `BASE + '/users'` is
/// the same prepend as `` `${BASE}/users` `` and must not answer differently just because a different
/// syntax wrote it — the two paths are explicitly isomorphic, so a head gate on one is a head gate on
/// both. The SECOND operand plays the role a template's second quasi plays: it is the `rest` shape gate,
/// and it is the empty string unless that operand itself resolves to visible literal text — so
/// `BASE + path` refuses the substitution for the same reason `` `${BASE}${path}` `` does. Later operands
/// are otherwise unchanged (project-wide `consts` only).
///
/// The isomorphism claimed above is the HEAD GATE's, and it is pinned by
/// `template_and_concat_answer_identically_for_every_head_shape`. One PRE-EXISTING asymmetry is not the
/// head gate's and is not closed here: the chain-level `has_literal_operand` check below rejects a chain
/// with no direct string-literal operand outright, so `BASE + (a ? '/x' : '/y')` yields nothing while the
/// template spelling fans out two head-dropped keys. That predates `same-file-const-prepend-v1` and errs
/// toward silence on both sides of the pair it affects; closing it needs its own measurement.
pub(super) fn resolve_concat_variants(
    arg: &Expr,
    consts: &HashMap<String, String>,
    locals: &LocalConsts,
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
    let mut pieces: Vec<TplPiece> = operands
        .iter()
        .map(|o| concat_operand_piece(o, consts, cm))
        .collect();
    let rest = operands
        .get(1)
        .and_then(|o| concat_operand_literal(o, consts, cm))
        .unwrap_or_default();
    if let Some(v) = locals.head_literal_for(operands[0], &rest) {
        pieces[0] = TplPiece::Fixed(v.to_string());
    }
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

/// One `+`-chain operand's VISIBLE LITERAL TEXT, or `None` when the operand names no literal — a string
/// literal, or an identifier/member that resolves in `consts` (the same lookup as `resolve_url_variants`'s
/// own `Expr::Ident|Member` arm). Split out from [`concat_operand_piece`] so the head shape gate and the
/// piece mapping read literalness through ONE predicate: a gate that re-derived "is this operand literal"
/// on its own could drift from what the pieces actually assemble.
fn concat_operand_literal(
    operand: &Expr,
    consts: &HashMap<String, String>,
    cm: &SourceMap,
) -> Option<String> {
    match unwrap_expr(operand) {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        e @ (Expr::Ident(_) | Expr::Member(_)) => consts.get(&expr_text(e, cm)).cloned(),
        _ => None,
    }
}

/// Map one `+`-chain operand to a [`TplPiece`]: visible literal text ([`concat_operand_literal`]) is
/// `Fixed`; a ternary with BOTH arms string literals is the fan-out `Slot`; anything else (an unresolved
/// identifier, a call, a nested non-string expression) is the old `Fixed("{}")` placeholder.
fn concat_operand_piece(
    operand: &Expr,
    consts: &HashMap<String, String>,
    cm: &SourceMap,
) -> TplPiece {
    if let Some(text) = concat_operand_literal(operand, consts, cm) {
        return TplPiece::Fixed(text);
    }
    match unwrap_expr(operand) {
        Expr::Cond(c) => match (resolve_cond_arm(&c.cons), resolve_cond_arm(&c.alt)) {
            (Some(cons), Some(alt)) => TplPiece::Slot(cons, alt),
            _ => TplPiece::Fixed("{}".to_string()),
        },
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
    fn str_concat_head_reads_a_gated_same_file_const() {
        // `const BASE = '/api'; axios.get(BASE + '/users')`. The project-wide `const_map_fragment` still
        // refuses bare undotted names (scope-insensitive lookup would let a common name shadow a function
        // param in an unrelated file and mis-key — never-guess), but the HEAD operand now consults this
        // file's own gated map (`same-file-const-prepend-v1`): BASE is bound exactly once here, never
        // reassigned, never shadowed, and initialized to a plain literal, so its value is READ. Before,
        // BASE -> `{}` and `consume_key_for`'s base-carrier head-drop threw the prefix away, keying
        // `GET /users` — right about the visible literal, but one prefix dimension short of the truth.
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "const BASE = '/api'; axios.get(BASE + '/users')",
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /api/users"));
    }

    #[test]
    fn str_concat_head_falls_back_to_head_drop_when_a_gate_fails() {
        // Param shadow: inside `send`, BASE is the parameter. The gate drops the name, and the old
        // head-drop residue (`{}` head + visible `/users`) stands — silence, not a wrong prefix.
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "const BASE = '/api';\nfunction send(BASE) { return axios.get(BASE + '/users'); }",
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /users"));
    }

    #[test]
    fn str_concat_head_only_later_operands_stay_opaque() {
        // Head-only, same as the template path: a same-file const in a LATER operand is a path-parameter
        // position and keeps its `{}` placeholder.
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "const V = 'v1'; axios.get('/api/' + V + '/users')",
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /api/{}/users"));
    }
}
