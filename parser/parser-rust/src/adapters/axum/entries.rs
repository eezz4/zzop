//! `RouterMountEntry` construction for the axum adapter — everything from one link of a builder chain
//! to the entries it yields, including the verb-expansion + dedup + entry-build cluster. Extracted from
//! `axum.rs` (file-size limit) in two steps, the second when the test-surface gate's documentation
//! pushed that file over the cap again. Deals in the adapter OUTPUT type (`RouterMountEntry`), unlike
//! the pure syn-expression helpers in `util.rs`.
//!
//! `axum.rs` keeps the other half — deciding WHICH statements are a router builder and which fragment
//! name their entries belong to, the half the test-surface gate acts on. Nothing here holds adapter
//! state, which is what made the split possible at this seam.

use syn::{Expr, ExprMethodCall};
use zzop_core::{ImportMap, RouterMountEntry};

use super::util::{collect_chain, simple_expr_ident, string_literal};
use super::VERB_METHODS;

/// Every entry one builder chain's links contribute, in chain order. An unrecognized method
/// (`.layer(...)`, `.with_state(...)`) contributes nothing and does not break the links after it.
pub(super) fn builder_entries(
    chain: &[&ExprMethodCall],
    imports: &ImportMap,
) -> Vec<RouterMountEntry> {
    let mut out = Vec::new();
    for mc in chain {
        match mc.method.to_string().as_str() {
            "route" => out.extend(route_entries(mc)),
            "nest" => out.extend(nest_entry(mc, imports)),
            "merge" => out.extend(merge_entry(mc, imports)),
            _ => {}
        }
    }
    out
}

fn route_entries(mc: &ExprMethodCall) -> Vec<RouterMountEntry> {
    let Some(path) = mc.args.first().and_then(string_literal) else {
        return Vec::new();
    };
    let Some(verb_expr) = mc.args.get(1) else {
        return Vec::new();
    };
    let (root, chain) = collect_chain(verb_expr);
    let Some((verb, handler, line)) = verb_call(root) else {
        return Vec::new(); // root isn't a recognized verb call — never guess the whole `.route()`
    };
    let mut out = Vec::new();
    push_verb(&mut out, verb, &path, handler, line);
    for link in chain {
        let name = link.method.to_string();
        if VERB_METHODS.contains(&name.as_str()) || name == "any" {
            let handler = link.args.first().and_then(simple_expr_ident);
            push_verb(
                &mut out,
                name.to_ascii_uppercase(),
                &path,
                handler,
                crate::line_of(&link.method),
            );
        }
    }
    out
}

fn verb_call(root: &Expr) -> Option<(String, Option<String>, u32)> {
    let Expr::Call(call) = root else { return None };
    let Expr::Path(p) = &*call.func else {
        return None;
    };
    let seg = p.path.segments.last()?;
    let verb = seg.ident.to_string();
    // `any(handler)` is axum's every-method catch-all — recognized here as the sentinel "ANY", expanded
    // to one entry per HTTP verb by `push_verb` below. `on(MethodFilter, handler)` (verb-from-argument)
    // stays out of v1 scope.
    if !VERB_METHODS.contains(&verb.as_str()) && verb != "any" {
        return None;
    }
    let handler = call.args.first().and_then(simple_expr_ident);
    Some((
        verb.to_ascii_uppercase(),
        handler,
        crate::line_of(&seg.ident),
    ))
}

fn nest_entry(mc: &ExprMethodCall, imports: &ImportMap) -> Option<RouterMountEntry> {
    let prefix = string_literal(mc.args.first()?)?;
    let ident = simple_expr_ident(mc.args.get(1)?)?;
    let specifier = imports.get(&ident).map(|b| b.specifier.clone());
    Some(RouterMountEntry::Mount {
        prefix,
        ident,
        specifier,
        attr_keys: Vec::new(),
    })
}

fn merge_entry(mc: &ExprMethodCall, imports: &ImportMap) -> Option<RouterMountEntry> {
    let ident = simple_expr_ident(mc.args.first()?)?;
    let specifier = imports.get(&ident).map(|b| b.specifier.clone());
    Some(RouterMountEntry::Mount {
        prefix: String::new(),
        ident,
        specifier,
        attr_keys: Vec::new(),
    })
}

/// Push one `Verb` entry per method for `method`, expanding the `any(...)` sentinel "ANY" to every
/// `HTTP_KEY_VERBS` verb (a catch-all binds one handler to all methods) and passing any concrete verb
/// through unchanged. Handler/line are shared across the expansion.
///
/// Deduped by (method, path): when a concrete verb co-occurs with a catch-all on the same path
/// (`.route("/x", get(h).any(h2))` → GET from both the concrete call and the `any` expansion), only the
/// FIRST push for a given verb survives — so the concrete handler is preserved and `duplicate-route`
/// doesn't see a phantom second GET /x from one `.route()` registration.
pub(super) fn push_verb(
    out: &mut Vec<RouterMountEntry>,
    method: String,
    path: &str,
    handler: Option<String>,
    line: u32,
) {
    if method == "ANY" {
        for v in zzop_core::HTTP_KEY_VERBS {
            push_unique(out, v.to_string(), path, handler.clone(), line);
        }
    } else {
        push_unique(out, method, path, handler, line);
    }
}

/// Push a `Verb` entry only if `out` has no entry with the same (method, path) yet.
fn push_unique(
    out: &mut Vec<RouterMountEntry>,
    method: String,
    path: &str,
    handler: Option<String>,
    line: u32,
) {
    let dup = out.iter().any(|e| {
        matches!(e, RouterMountEntry::Verb { method: m, path: p, .. } if *m == method && p == path)
    });
    if !dup {
        out.push(verb_entry(method, path, handler, line));
    }
}

fn verb_entry(method: String, path: &str, handler: Option<String>, line: u32) -> RouterMountEntry {
    RouterMountEntry::Verb {
        method,
        path: path.to_string(),
        handler,
        line,
        attr_keys: Vec::new(),
    }
}
