//! URL-argument resolution: literal / const-indirection / template-literal / top-level-ternary
//! variants (`cond-literal-fanout-v1`), plus the shared [`TplPiece`] vocabulary and text helpers.

use std::collections::{HashMap, HashSet};

use swc_core::common::{SourceMap, SourceMapper, Spanned};
use swc_core::ecma::ast::{Expr, Lit, MemberProp, Tpl};

use super::concat::resolve_concat_variants;

/// Resolve a URL argument to every syntactically-possible path string ("variants"); an empty vec means
/// dynamic/unresolvable, same meaning as the old `None`. A plain literal or const indirection yields
/// exactly one variant, unchanged from before. A top-level ternary whose BOTH arms are string literals
/// fans out to one variant per arm (cons first, then alt), deduped preserving first-seen order — visible
/// literal enumeration, not a guess (`cond-literal-fanout-v1`); any other ternary shape is unresolved.
/// See [`resolve_template_variants`] for the template
/// literal case, which fans out per-interpolation with its own cap, and [`resolve_concat_variants`] for
/// the binary `+` string-concatenation case (`str-concat-url-v1`).
pub(super) fn resolve_url_variants(
    arg: &Expr,
    consts: &HashMap<String, String>,
    cm: &SourceMap,
) -> Vec<String> {
    match arg {
        Expr::Lit(Lit::Str(s)) => vec![s.value.as_str().unwrap_or_default().to_string()],
        Expr::Cond(c) => match (resolve_cond_arm(&c.cons), resolve_cond_arm(&c.alt)) {
            (Some(cons), Some(alt)) => dedup_preserve_order(vec![cons, alt]),
            _ => Vec::new(),
        },
        Expr::Tpl(t) => resolve_template_variants(t),
        Expr::Ident(_) | Expr::Member(_) => consts
            .get(&expr_text(arg, cm))
            .cloned()
            .into_iter()
            .collect(),
        Expr::Bin(_) => resolve_concat_variants(arg, consts, cm),
        _ => Vec::new(),
    }
}

/// One piece of a template literal between two quasis: either fixed text (the old `"{}"` placeholder),
/// or a conditional-literal fan-out slot carrying its two literal arm values.
pub(super) enum TplPiece {
    Fixed(String),
    Slot(String, String),
}

/// Resolve one ternary arm to a single fan-out string, or `None` if it is not a bounded single-variant
/// value. A plain string literal is its own value; a template literal is accepted only when it resolves to
/// EXACTLY ONE variant (`` `/${slug}` `` -> `"/{}"`, `` `/x` `` -> `"/x"`) — a template that itself fans
/// out (a nested ternary) would multiply the outer 2-arm slot beyond its bound, so it falls through to the
/// fixed `{}` placeholder. This lets a slash INSIDE a branch (`` slug ? `/${slug}` : '' ``) survive.
pub(super) fn resolve_cond_arm(e: &Expr) -> Option<String> {
    match e {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        Expr::Tpl(t) => match resolve_template_variants(t).as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Template literal -> URL variants. `/api/users/${id}` -> `["/api/users/{}"]`, same as before.
/// (`cond-literal-fanout-v1`): an interpolation whose expression is a ternary with BOTH arms resolving to
/// a single value (via [`resolve_cond_arm`] — a string literal, or a template like `` `/${slug}` `` that
/// resolves to one variant `"/{}"`) is a fan-out slot instead of a fixed `{}` — e.g.
/// `` `/users${isRegister ? '' : '/login'}` `` -> `["/users", "/users/login"]`, and
/// `` `/articles${slug ? `/${slug}` : ''}` `` -> `["/articles/{}", "/articles"]` (the in-branch slash is
/// kept). Multiple slots cartesian-product together, capped at 2 slots (<=4 variants); a 3rd+ slot forces
/// EVERY slot in this template back to the old fixed `{}` behavior, keeping output bounded and
/// deterministic. Variants are deduped preserving first-seen order.
fn resolve_template_variants(t: &Tpl) -> Vec<String> {
    let mut pieces: Vec<TplPiece> = Vec::with_capacity(t.exprs.len());
    let mut slot_count = 0usize;
    for e in t.exprs.iter() {
        if let Expr::Cond(c) = &**e {
            if let (Some(cons), Some(alt)) = (resolve_cond_arm(&c.cons), resolve_cond_arm(&c.alt)) {
                slot_count += 1;
                pieces.push(TplPiece::Slot(cons, alt));
                continue;
            }
        }
        pieces.push(TplPiece::Fixed("{}".to_string()));
    }
    if slot_count > 2 {
        // Bounded-output cap: fall back to fixed "{}" for every slot in this template.
        for p in &mut pieces {
            if matches!(p, TplPiece::Slot(_, _)) {
                *p = TplPiece::Fixed("{}".to_string());
            }
        }
    }

    let mut variants = vec![String::new()];
    for (i, q) in t.quasis.iter().enumerate() {
        let quasi_text = q
            .cooked
            .as_ref()
            .and_then(|a| a.as_str())
            .unwrap_or_default();
        for v in variants.iter_mut() {
            v.push_str(quasi_text);
        }
        if i < t.exprs.len() {
            match &pieces[i] {
                TplPiece::Fixed(s) => {
                    for v in variants.iter_mut() {
                        v.push_str(s);
                    }
                }
                TplPiece::Slot(cons, alt) => {
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

/// Dedup a variant list, preserving first-seen order (`cond-literal-fanout-v1`'s "same literal on both
/// arms" and "same variant produced twice" cases must collapse to one).
pub(super) fn dedup_preserve_order(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

/// Reconstruct a dotted access expression's text ("ControlKey.AUTHEN.getUserInfo"); fall back to the source slice.
pub(super) fn expr_text(node: &Expr, cm: &SourceMap) -> String {
    match node {
        Expr::Ident(id) => id.sym.to_string(),
        Expr::Member(m) => {
            if let MemberProp::Ident(name) = &m.prop {
                format!("{}.{}", expr_text(&m.obj, cm), name.sym)
            } else {
                cm.span_to_snippet(m.span).unwrap_or_default()
            }
        }
        _ => cm.span_to_snippet(node.span()).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::egress::{extract_http_egress, files, keys};

    // --- conditional-literal fan-out (`cond-literal-fanout-v1`) ---

    #[test]
    fn template_conditional_literal_interpolation_fans_out_the_url() {
        let out = extract_http_egress(&files(&[(
            "conduit.ts",
            "axios.post(`/users${isRegister ? '' : '/login'}`, body);",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("POST /users".to_string()),
                Some("POST /users/login".to_string()),
            ]
        );
        assert!(out.iter().all(|c| c.raw.is_none() && c.method.is_none()));
    }

    #[test]
    fn top_level_ternary_url_argument_fans_out() {
        let out = extract_http_egress(&files(&[("a.ts", "axios.get(cond ? '/a' : '/b');")]));
        assert_eq!(
            keys(&out),
            vec![Some("GET /a".to_string()), Some("GET /b".to_string())]
        );
    }

    #[test]
    fn template_ternary_with_a_template_arm_keeps_the_in_branch_slash() {
        // fe-vite Editor.jsx shape: the slash lives INSIDE the cons branch (`` `/${slug}` ``). It used to
        // collapse to a malformed `/articles{}`; now the template arm resolves to `/{}` and fans out,
        // keeping the slash — `/articles/{}` (has-slug) and `/articles` (no-slug).
        let out = extract_http_egress(&files(&[(
            "a.jsx",
            "axios.put(`/articles${slug ? `/${slug}` : ''}`);",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("PUT /articles/{}".to_string()),
                Some("PUT /articles".to_string()),
            ]
        );
    }

    #[test]
    fn correlated_verb_and_url_ternaries_pair_instead_of_cross_producting() {
        // fe-vite Editor.jsx: the verb ternary and the url ternary share the SAME guard `slug`, so only
        // the two reachable branches exist — PUT /articles/{} (slug truthy) and POST /articles (falsy).
        // The two cross combos (POST /articles/{}, PUT /articles) are unreachable and must NOT be emitted
        // (they used to cascade into spurious method-mismatch findings).
        let out = extract_http_egress(&files(&[(
            "Editor.jsx",
            "axios[slug ? 'put' : 'post'](`/articles${slug ? `/${slug}` : ''}`, { article });",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("PUT /articles/{}".to_string()),
                Some("POST /articles".to_string()),
            ]
        );
    }

    #[test]
    fn independent_verb_and_url_ternaries_still_cross_product() {
        // DIFFERENT guards (`a` vs `b`) are genuinely independent — the full Cartesian product is the
        // correct over-approximation, unchanged by the correlation special-case.
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "axios[a ? 'put' : 'post'](`/x${b ? '/1' : '/2'}`);",
        )]));
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn top_level_ternary_with_a_template_arm_fans_out() {
        let out = extract_http_egress(&files(&[("a.ts", "axios.get(cond ? `/a/${x}` : '/b');")]));
        assert_eq!(
            keys(&out),
            vec![Some("GET /a/{}".to_string()), Some("GET /b".to_string())]
        );
    }

    #[test]
    fn template_ternary_interpolation_with_one_non_literal_arm_keeps_the_old_placeholder() {
        let out = extract_http_egress(&files(&[("a.ts", "axios.get(`/x${cond ? a : 'b'}`);")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /x{}"));
    }

    #[test]
    fn more_than_two_conditional_literal_interpolations_falls_back_to_placeholders_for_all() {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "axios.get(`/x${a ? '1' : '2'}/y${b ? '3' : '4'}/z${c ? '5' : '6'}`);",
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /x{}/y{}/z{}"));
    }

    #[test]
    fn top_level_ternary_with_identical_arms_dedups_to_one_consume() {
        let out = extract_http_egress(&files(&[("a.ts", "axios.get(cond ? '/same' : '/same');")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /same"));
    }

    #[test]
    fn ternary_with_one_keying_and_one_vetoed_arm_emits_the_key_plus_an_unresolved_consume() {
        // Mixed partial-veto: '/feed' keys, '?public' veto-lists out of every bucket (query-only URL
        // names no path). The keyed variant is emitted AND the vetoed variant falls back to the
        // unresolved shape (raw = the whole ternary's source text, method carried) — strictly additive
        // over the pre-fanout behavior, which emitted only the unresolved consume.
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "axios.get(cond ? '/feed' : '?public');",
        )]));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].key.as_deref(), Some("GET /feed"));
        assert!(out[0].raw.is_none());
        assert!(out[1].key.is_none());
        assert_eq!(out[1].raw.as_deref(), Some("cond ? '/feed' : '?public'"));
        assert_eq!(out[1].method.as_deref(), Some("GET"));
    }

    // --- generated-SDK receivers are not recognized (decision: generated SDKs are injection
    // adapters, not engine vocab — the former oazapfts-specific recognition lived here) ---

    #[test]
    fn former_qs_suffix_special_case_is_gone_trailing_interpolation_is_a_plain_placeholder() {
        // A trailing `${QS.query(...)}`-shaped interpolation used to be dropped entirely as
        // oazapfts-codegen's query-string suffix. That special case is gone: it now keys like any other
        // trailing interpolation, as an ordinary `{}` placeholder.
        let out = extract_http_egress(&files(&[(
            "activity.ts",
            r#"axios.get(`/activities${QS.query(QS.explode({ albumId }))}`);"#,
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /activities{}"));
    }
}
