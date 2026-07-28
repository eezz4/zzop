//! URL-argument resolution: literal / const-indirection / template-literal / top-level-ternary
//! variants (`cond-literal-fanout-v1`) and the same-file constant rules
//! (`same-file-const-prepend-v1` at the head, `same-file-url-binding-v1` at the whole argument — gates in
//! [`super::local_consts`]), plus the shared [`TplPiece`] vocabulary and text helpers.
//! Tests live in the sibling `url_resolve_tests.rs` (300-line file budget).

use std::collections::{HashMap, HashSet};

use swc_core::common::{SourceMap, SourceMapper, Spanned};
use swc_core::ecma::ast::{Expr, Lit, MemberProp, Tpl};

use super::concat::resolve_concat_variants;
use super::local_consts::{quasi_text, tpl_starts_with_interpolation, LocalConsts};

/// Resolve a URL argument to every syntactically-possible path string ("variants"); an empty vec means
/// dynamic/unresolvable, same meaning as the old `None`. A plain literal or const indirection yields
/// exactly one variant, unchanged from before. A top-level ternary whose BOTH arms are string literals
/// fans out to one variant per arm (cons first, then alt), deduped preserving first-seen order — visible
/// literal enumeration, not a guess (`cond-literal-fanout-v1`); any other ternary shape is unresolved.
/// See [`resolve_template_variants`] for the template
/// literal case, which fans out per-interpolation with its own cap, and [`resolve_concat_variants`] for
/// the binary `+` string-concatenation case (`str-concat-url-v1`).
///
/// `locals` is THIS file's same-file URL knowledge ([`super::local_consts`]), read at exactly two
/// positions: a URL's leading slot (`same-file-const-prepend-v1`) and a bare identifier standing alone as
/// the WHOLE url argument (`axios.get(BASE)` — `same-file-url-binding-v1`). The project-wide dotted
/// `consts` map is consulted FIRST at the identifier/member arm, so nothing this file says can displace a
/// dotted constant that already resolved.
pub(super) fn resolve_url_variants(
    arg: &Expr,
    consts: &HashMap<String, String>,
    locals: &LocalConsts,
    cm: &SourceMap,
) -> Vec<String> {
    match arg {
        Expr::Lit(Lit::Str(s)) => vec![s.value.as_str().unwrap_or_default().to_string()],
        Expr::Cond(c) => match (resolve_cond_arm(&c.cons), resolve_cond_arm(&c.alt)) {
            (Some(cons), Some(alt)) => dedup_preserve_order(vec![cons, alt]),
            _ => Vec::new(),
        },
        Expr::Tpl(t) => resolve_template_variants(t, locals),
        Expr::Ident(_) | Expr::Member(_) => match consts.get(&expr_text(arg, cm)) {
            Some(v) => vec![v.clone()],
            None => locals.whole_url_for(arg).unwrap_or_default().to_vec(),
        },
        Expr::Bin(_) => resolve_concat_variants(arg, consts, locals, cm),
        // `same-file-fn-url-v1`: `fetch(chargesUrl())` — a zero-argument call on a same-file helper whose
        // whole body is one `return <resolvable expr>`. Gated entirely in [`super::binding_census`] and
        // [`LocalConsts`]; a call with arguments, a method call, or an imported helper resolves to
        // nothing here exactly as before.
        Expr::Call(_) => locals.whole_url_for(arg).unwrap_or_default().to_vec(),
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
/// A template nested inside a ternary arm is resolved with NO locals map. A leading-interpolation arm DOES
/// reach here — both `axios.get(c ? `${B}/x` : '/y')` and `` `${c ? `${B}/x` : '/y'}/z` `` are real shapes —
/// so this is a deliberate refusal, not an unreachable branch: a fan-out arm's own head has no measured
/// case behind it, and withholding the map keeps every arm resolving exactly as it did before
/// `same-file-const-prepend-v1`. The arm falls back to `{}`, i.e. silence, never a wrong value.
pub(super) fn resolve_cond_arm(e: &Expr) -> Option<String> {
    match e {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        Expr::Tpl(t) => match resolve_template_variants(t, &LocalConsts::default()).as_slice() {
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
///
/// (`same-file-const-prepend-v1`): when the template STARTS with its first interpolation and that
/// expression is a bare identifier bound to a same-file top-level string literal that clears all four
/// gates in [`super::local_consts`], the head piece is that literal instead of `{}` — the base is read
/// rather than dropped. The SECOND quasi is handed to [`head_literal_for`] as the shape gate: an empty one
/// means a `{}` would be glued straight onto the head, which it refuses. Head only; see that function's
/// doc for why a mid-path slot must stay `{}` and which harms survive the substitution.
fn resolve_template_variants(t: &Tpl, locals: &LocalConsts) -> Vec<String> {
    let mut pieces: Vec<TplPiece> = Vec::with_capacity(t.exprs.len());
    let mut slot_count = 0usize;
    let head_literal = tpl_starts_with_interpolation(t)
        .then(|| {
            t.exprs
                .first()
                .and_then(|e| locals.head_literal_for(e, quasi_text(t, 1)))
        })
        .flatten()
        .map(str::to_string);
    for (i, e) in t.exprs.iter().enumerate() {
        if i == 0 {
            if let Some(v) = &head_literal {
                pieces.push(TplPiece::Fixed(v.clone()));
                continue;
            }
        }
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
