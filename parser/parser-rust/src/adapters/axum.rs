//! axum router PROVIDES, projected as framework-neutral router-mount fragments — the same shape
//! `zzop_parser_python_3::adapters::fastapi` emits. See `zzop_core::fragments`' module doc for the
//! fragment shape rationale.
//!
//! ## Scope (v1)
//! Import-gated on `axum` (specifier `"axum"` or `"axum::<...>"`). Recognition walks each TOP-LEVEL
//! function's OWN block statements one level deep (not a closure/nested fn/`impl` method) — unlike
//! `lang::symbols`'s stricter file-top-level-only scope, since axum apps idiomatically build their router
//! inside `fn main()` or a small `fn app() -> Router` helper.
//!
//! - **Builder chains**: a method-call chain rooted at `Router::new()` (bare or `axum::`-qualified) is
//!   recognized as, and appends to, one `RouterMountFragment` named after the receiver, in three shapes:
//!   `let app = Router::new()...;` (fresh); `let app = app.route(...);` (shadowing re-`let`) or
//!   `app = app.route(...);` (plain reassignment, needs an earlier `let mut app = ...`) — a chain rooted
//!   at reading the SAME name being bound; or a bare `Router::new()...` chain with no binding at all in a
//!   function's own TAIL position (no trailing `;`) — named after the ENCLOSING FUNCTION, since there is
//!   no receiver ident (a mid-body `return Router::new()...;` is NOT recognized — only the
//!   trivially-visible tail case is). Fragment names are tracked FILE-GLOBALLY, not per-function: two
//!   different top-level functions each locally binding the same variable name have their entries merged
//!   — a rare pattern, documented rather than engineered around, mirroring `adapters::fastapi`'s equally
//!   simple file-global receiver-name model.
//! - **Verbs**: `.route("<path>", get(handler)...verb(handler2)...)` — path is the LITERAL first
//!   argument (non-literal skips the WHOLE `.route()` call); the second argument is itself a chain rooted
//!   at one of axum's `get`/`post`/`put`/`delete`/`patch` verb functions, each link (root + every chained
//!   verb) becoming one `Verb{method: UPPERCASE, path, handler, line, attr_keys: vec![]}`. `handler` is
//!   `Some(name)` only for a bare function-path argument (`get(h)`); a closure/call leaves it `None`, but
//!   the entry is still emitted. Both `:id` and `{id}` pass through the raw literal untouched.
//! - **Mounts**: `.nest("<prefix>", child)` -> `Mount{prefix: <literal>, ident: <child's bare name>,
//!   specifier: <ImportMap specifier for ident, else None>, attr_keys: vec![]}`; a non-literal prefix or
//!   non-identifier child skips that call's entry entirely. `.merge(child)` -> same shape, `prefix: ""`.
//! - Any other chained method (`.layer(...)`, `.with_state(...)`, ...) is silently skipped — no
//!   middleware/`layer` auth-attribute recognition here (M3 scope, out of bounds).
//! - One `RouterMountFragment` per name with >=1 surviving entry, in first-appearance order.

use std::collections::HashMap;
use syn::{Expr, ExprAssign, ExprMethodCall, ItemFn, Lit, Local, Pat, Stmt};
use zzop_core::{ImportMap, RouterMountEntry, RouterMountFragment};

pub(crate) const VERB_METHODS: &[&str] = &["get", "post", "put", "delete", "patch"];

/// Extract this file's axum router-mount fragments — see module doc. Empty on parse failure, and
/// whenever the file does not import `axum` (never panics).
pub fn extract_axum_router_fragments(_rel: &str, text: &str) -> Vec<RouterMountFragment> {
    let Some(file) = crate::parse_file(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    if !imports_axum(&imports) {
        return Vec::new();
    }

    let mut order: Vec<String> = Vec::new();
    let mut entries: HashMap<String, Vec<RouterMountEntry>> = HashMap::new();
    for item in &file.items {
        if let syn::Item::Fn(f) = item {
            scan_fn(f, &imports, &mut order, &mut entries);
        }
    }
    order
        .into_iter()
        .filter_map(|name| {
            let es = entries.remove(&name)?;
            if es.is_empty() {
                return None;
            }
            Some(RouterMountFragment { name, entries: es })
        })
        .collect()
}

fn imports_axum(imports: &ImportMap) -> bool {
    imports
        .values()
        .any(|b| b.specifier == "axum" || b.specifier.starts_with("axum::"))
}

fn scan_fn(
    f: &ItemFn,
    imports: &ImportMap,
    order: &mut Vec<String>,
    entries: &mut HashMap<String, Vec<RouterMountEntry>>,
) {
    for stmt in &f.block.stmts {
        match stmt {
            Stmt::Local(local) => scan_local(local, imports, order, entries),
            Stmt::Expr(Expr::Assign(a), _) => scan_assign(a, imports, order, entries),
            _ => {}
        }
    }
    if let Some(Stmt::Expr(tail, None)) = f.block.stmts.last() {
        let (root, chain) = collect_chain(tail);
        if is_router_new_call(root) {
            append(
                order,
                entries,
                f.sig.ident.to_string(),
                builder_entries(&chain, imports),
            );
        }
    }
}

fn scan_local(
    local: &Local,
    imports: &ImportMap,
    order: &mut Vec<String>,
    entries: &mut HashMap<String, Vec<RouterMountEntry>>,
) {
    let Some(name) = simple_pat_ident(&local.pat) else {
        return;
    };
    let Some(init) = &local.init else { return };
    let (root, chain) = collect_chain(&init.expr);
    if !accepts_chain_root(root, &name, entries) {
        return;
    }
    append(order, entries, name, builder_entries(&chain, imports));
}

fn scan_assign(
    a: &ExprAssign,
    imports: &ImportMap,
    order: &mut Vec<String>,
    entries: &mut HashMap<String, Vec<RouterMountEntry>>,
) {
    let Some(name) = simple_expr_ident(&a.left) else {
        return;
    };
    let (root, chain) = collect_chain(&a.right);
    if !accepts_chain_root(root, &name, entries) {
        return;
    }
    append(order, entries, name, builder_entries(&chain, imports));
}

fn accepts_chain_root(
    root: &Expr,
    name: &str,
    entries: &HashMap<String, Vec<RouterMountEntry>>,
) -> bool {
    is_router_new_call(root) || (entries.contains_key(name) && is_same_ident(root, name))
}

fn append(
    order: &mut Vec<String>,
    entries: &mut HashMap<String, Vec<RouterMountEntry>>,
    name: String,
    new_entries: Vec<RouterMountEntry>,
) {
    if new_entries.is_empty() {
        return;
    }
    if !entries.contains_key(&name) {
        order.push(name.clone());
    }
    entries.entry(name).or_default().extend(new_entries);
}

/// Decomposes a method-call chain into its root expression and the ordered list of chained calls —
/// `Router::new().route(a).nest(b)` -> `(Router::new(), [.route(a), .nest(b)])`.
fn collect_chain(expr: &Expr) -> (&Expr, Vec<&ExprMethodCall>) {
    match expr {
        Expr::MethodCall(mc) => {
            let (root, mut chain) = collect_chain(&mc.receiver);
            chain.push(mc);
            (root, chain)
        }
        other => (other, Vec::new()),
    }
}

fn builder_entries(chain: &[&ExprMethodCall], imports: &ImportMap) -> Vec<RouterMountEntry> {
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
    let mut out = vec![verb_entry(verb, &path, handler, line)];
    for link in chain {
        let name = link.method.to_string();
        if VERB_METHODS.contains(&name.as_str()) {
            let handler = link.args.first().and_then(simple_expr_ident);
            out.push(verb_entry(
                name.to_ascii_uppercase(),
                &path,
                handler,
                crate::line_of(&link.method),
            ));
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
    if !VERB_METHODS.contains(&verb.as_str()) {
        return None;
    }
    let handler = call.args.first().and_then(simple_expr_ident);
    Some((
        verb.to_ascii_uppercase(),
        handler,
        crate::line_of(&seg.ident),
    ))
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

fn is_router_new_call(expr: &Expr) -> bool {
    let Expr::Call(call) = expr else { return false };
    let Expr::Path(p) = &*call.func else {
        return false;
    };
    let segs = &p.path.segments;
    let n = segs.len();
    n >= 2 && segs[n - 1].ident == "new" && segs[n - 2].ident == "Router"
}

fn is_same_ident(expr: &Expr, name: &str) -> bool {
    simple_expr_ident(expr).as_deref() == Some(name)
}

fn simple_expr_ident(expr: &Expr) -> Option<String> {
    let Expr::Path(p) = expr else { return None };
    if p.path.segments.len() != 1 {
        return None;
    }
    Some(p.path.segments[0].ident.to_string())
}

fn simple_pat_ident(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(pi) => Some(pi.ident.to_string()),
        _ => None,
    }
}

fn string_literal(expr: &Expr) -> Option<String> {
    let Expr::Lit(el) = expr else { return None };
    let Lit::Str(s) = &el.lit else { return None };
    Some(s.value())
}

#[cfg(test)]
mod tests;
