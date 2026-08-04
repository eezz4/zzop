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
//!
//! ## Test surface is excluded (2026-08-02 — the last adapter in this crate to gate it)
//! A test fixture's `Router::new().route("/admin/reset", post(h))` is not a DEPLOYED route, and until
//! this batch it entered the cross-layer join as one. The same three gates `adapters::raw_sql` and
//! `adapters::http_clients` document apply here, through the same predicate: `zzop_core::is_test_file`
//! on the path, the file's own `#![cfg(test)]` inner attributes, and a subtree skip on every test-gated
//! item.
//!
//! The third gate asks about ONE node axis where the siblings ask about three (`Item`, `ImplItem`,
//! `TraitItem`), because the other two are unreachable from here. Both siblings are `syn::visit::Visit`
//! walks that descend the whole file, while this one reads `syn::File::items` and scans only
//! `Item::Fn` — nothing inside an `impl`, a `trait`, or even a nested `mod` is ever looked at (the v1
//! scope above). Measured 2026-08-02 before any gate existed: `#[cfg(test)] mod tests`, `#[test] fn` in
//! an `impl`, and a `#[cfg(test)]` default trait method each already yielded zero fragments, while a
//! file-level `#![cfg(test)]`, a top-level `#[cfg(test)] fn`/`#[test] fn`/`#[cfg(all(test, not(miri)))]
//! fn`, and a `tests/` PATH each leaked a deployed route. `OUT_OF_REACH_ROUTERS` in this module's tests
//! pins that measurement, so the missing axes stay a proven non-gap rather than an asserted one. The
//! gate is nonetheless applied to EVERY top-level item rather than to `Item::Fn` alone, so it stays
//! co-extensive with the loop should the walk ever widen.
//!
//! What is NOT gated is [`crate::lang::imports::parse_imports`] and the `imports_axum` check built on
//! it, for the reason `adapters::http_clients` keeps its `BindingCollector` file-wide: a file whose only
//! `use axum::...` is itself `#[cfg(test)]`-gated can still ship a fully-qualified
//! `axum::Router::new().route(...)`, and narrowing the import scan would delete that REAL route instead
//! of suppressing a fixture — the losing direction. Measured: with the import scan left flat, that file
//! still yields its shipped route.
//!
//! Two residuals, both shared with `lang::test_spans` rather than introduced here. A `#[cfg(test)]`
//! attribute on a LOCAL STATEMENT (`fn ship() { #[cfg(test)] let app = Router::new()…; }`) is invisible
//! to both — `test_spans` walks items, not statements, so it records no span there either; the two axes
//! agree, and making this adapter stricter alone would emit a suppression no rule pack could subtract.
//! And the file-global fragment-name map means a chain rooted at a bare ident is accepted whenever ANY
//! earlier item registered that name, so gating an item could in principle strand a later statement that
//! was leaning on it — unreachable in compilable Rust, where such a chain root must resolve to a local
//! of the SAME function, hence the same item, hence gated or kept as one.

use std::collections::HashMap;
use syn::{Expr, ExprAssign, ItemFn, Local, Stmt};
use zzop_core::{ImportMap, RouterMountEntry, RouterMountFragment};

use crate::lang::test_spans::{is_test_gated, item_is_test_gated};

mod entries;
mod util;
use entries::builder_entries;
use util::{collect_chain, is_router_new_call, is_same_ident, simple_expr_ident, simple_pat_ident};

pub(crate) const VERB_METHODS: &[&str] = &["get", "post", "put", "delete", "patch"];

/// Extract this file's axum router-mount fragments — see module doc. Empty on parse failure, for a test
/// file or a test-gated one, and whenever the file does not import `axum` (never panics).
pub fn extract_axum_router_fragments(rel: &str, text: &str) -> Vec<RouterMountFragment> {
    // Test surface — see the module doc's "Test surface is excluded". Path first (no parse needed).
    if zzop_core::is_test_file(rel) {
        return Vec::new();
    }
    let Some(file) = crate::parse_file(text) else {
        return Vec::new();
    };
    if is_test_gated(&file.attrs) {
        return Vec::new();
    }
    // Deliberately NOT narrowed to non-test items — see the module doc's paragraph on `imports_axum`.
    let imports = crate::lang::imports::parse_imports(text);
    if !imports_axum(&imports) {
        return Vec::new();
    }

    let mut order: Vec<String> = Vec::new();
    let mut entries: HashMap<String, Vec<RouterMountEntry>> = HashMap::new();
    for item in &file.items {
        if item_is_test_gated(item) {
            continue; // a fixture's routes are not deployed PROVIDES
        }
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

#[cfg(test)]
mod tests;
