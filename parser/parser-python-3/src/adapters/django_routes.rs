//! Django URLconf route PROVIDES, projected as the SAME framework-neutral router-mount fragments
//! `adapters::fastapi` emits (see that module's doc for the fragment-shape rationale — a router split
//! across `include()` mounts can't be resolved to a real URL from one file alone). The engine's
//! `analyze::compose_router_mount_provides` composes these into whole-tree `IoProvide`s exactly as it
//! does the FastAPI ones — no Django-specific composition path.
//!
//! ## UNKNOWN_VERB, by construction
//! A Django `url()`/`re_path()`/`path()` registers a PATH bound to a VIEW; the HTTP method(s) are
//! decided inside the view class (its `get`/`post`/... handlers, or a DRF mixin set), never at the
//! URLconf. A single-file URLconf scan therefore cannot honestly name a verb, so every route emits the
//! `zzop_core::UNKNOWN_VERB` sentinel (`"?"`) rather than a fabricated GET/POST — the same verb-unknown
//! doctrine `crate::file_routes`'s serve-all handler uses. At assemble time the engine partitions these
//! sentinels into the path-level "served, verb-unknown" set and drives the
//! `cross-layer/unknown-verb-route` disclosure; they never join by exact method.
//!
//! ## Scope (v1 — decision-free literal slice)
//! Import-gated on `django.urls` / `django.conf.urls` (a file that imports neither yields no fragments,
//! never a bare-name guess). Recognition is restricted to a TOP-LEVEL `urlpatterns = [ ... ]` list
//! assignment; each list element is one `url`/`re_path`/`path` call:
//! - `url(r'<regex>', <View>.as_view())` / `re_path(...)` -> `Verb{method: UNKNOWN_VERB, path: <regex
//!   reduced to a literal-with-`{}` key>, handler: <View name>}`. A regex that does not cleanly reduce
//!   to a literal path (alternation, unnamed groups, a character class outside a named-group param
//!   position, a quantifier) is SKIPPED — never a guessed key.
//! - `path('<route>', <View>.as_view())` -> same `Verb`, with `<int:pk>`/`<slug:s>`/`<name>` converters
//!   reduced to `{}`.
//! - `url(r'^<prefix>/', include('<dotted.module>'))` / `path('<prefix>/', include('...'))` ->
//!   `Mount{prefix, ident: <last dotted segment>, specifier: Some(<full dotted module path>)}`. Django's
//!   `include()` takes a STRING dotted-module path (unlike FastAPI's imported-name form); the engine
//!   resolves that string via the same `resolve_python_import` candidate builder the FastAPI mount uses.
//!
//! ## Deferred (v1 limitation, honest silence)
//! `include(router.urls)` (a DRF `DefaultRouter` mount, non-string argument) and `router.register(...)`
//! ViewSet registrations are OUT OF SCOPE — expanding a ViewSet's CRUD route set needs base-class
//! verb-set judgment this decision-free slice does not make. Such an entry is skipped silently: the
//! router's routes are honestly absent, never a guessed mount.

use std::collections::HashMap;

use ruff_python_ast::{Expr, ExprList, Stmt};
use zzop_core::{ImportMap, RouterMountEntry, RouterMountFragment, UNKNOWN_VERB};

/// The `django.urls`/`django.conf.urls` local-name -> original-name map for this file (empty-but-`Some`
/// when the modules are imported only as a namespace, e.g. `import django.urls`). `None` = import gate
/// failed (no Django URLconf import at all).
type DjangoFns = HashMap<String, String>;

/// `rel` -> the file's Python dotted-module path (`conduit/apps/articles/urls.py` ->
/// `conduit.apps.articles.urls`, `pkg/__init__.py` -> `pkg`): the fragment name AND what an
/// `include('<dotted>')` mount's `ident` equals, so the composer's root-exclusion-by-name matches a
/// mounted child even when its `include` specifier fails to resolve (a truncated-prefix route would
/// otherwise be fabricated), and so the `ident` is UNIQUE (never the bare `urls` segment that every
/// URLconf shares, which would collide across files/languages in the composer's flat ident set).
/// Assumes the scan root is the Python package root (the normal layout); a non-package-root scan makes
/// the derived path diverge from the `include` string, which degrades to the pre-existing
/// unresolvable-mount limitation (honest miss, never a wrong key).
fn rel_to_module_path(rel: &str) -> String {
    let stem = rel
        .strip_suffix("/__init__.py")
        .or_else(|| rel.strip_suffix(".py"))
        .unwrap_or(rel);
    stem.replace('/', ".")
}

/// Extract this file's Django URLconf router-mount fragments — see module doc. Returns an empty vec on
/// parse failure and whenever the file imports no Django URLconf module (never panics).
pub fn extract_django_route_fragments(rel: &str, text: &str) -> Vec<RouterMountFragment> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    let Some(fns) = django_url_fns(&imports) else {
        return Vec::new();
    };
    let Some(list) = find_urlpatterns(&module.body) else {
        return Vec::new();
    };
    let idx = crate::LineIndex::new(text);

    let mut entries: Vec<RouterMountEntry> = Vec::new();
    for elt in &list.elts {
        if let Some(entry) = match_route_element(elt, &fns, &idx) {
            entries.push(entry);
        }
    }
    if entries.is_empty() {
        return Vec::new();
    }
    vec![RouterMountFragment {
        name: rel_to_module_path(rel),
        entries,
    }]
}

/// The import gate + classifier substrate: `Some(map)` (local -> original) when the file imports
/// `django.urls`/`django.conf.urls` in any form, else `None`. Only Django-URLconf bindings enter the
/// map; other imports are irrelevant to route classification.
fn django_url_fns(imports: &ImportMap) -> Option<DjangoFns> {
    let mut gated = false;
    let mut map = DjangoFns::new();
    for (local, b) in imports.iter() {
        let s = b.specifier.as_str();
        if s == "django.urls"
            || s == "django.conf.urls"
            || s.starts_with("django.urls.")
            || s.starts_with("django.conf.urls.")
        {
            gated = true;
            if b.original != "*" {
                map.insert(local.clone(), b.original.clone());
            }
        }
    }
    gated.then_some(map)
}

/// The top-level `urlpatterns = [ ... ]` list value, if present (single bare-`Name` target only). A
/// `urlpatterns += [...]` / `urlpatterns = func(...)` shape is out of scope v1.
fn find_urlpatterns(body: &[Stmt]) -> Option<&ExprList> {
    for stmt in body {
        let Stmt::Assign(a) = stmt else { continue };
        if a.targets.len() != 1 {
            continue;
        }
        let Expr::Name(target) = &a.targets[0] else {
            continue;
        };
        if target.id.as_str() != "urlpatterns" {
            continue;
        }
        if let Expr::List(list) = &*a.value {
            return Some(list);
        }
    }
    None
}

/// The route-path shape a `url`/`re_path`/`path` element carries as its first argument.
#[derive(Clone, Copy)]
enum Kind {
    /// `url`/`re_path` — a regex reduced by [`regex_to_key`].
    Regex,
    /// `path` — a route string reduced by [`path_to_key`].
    Path,
}

/// One `urlpatterns` list element -> a mount entry, or `None` for any non-qualifying shape (see module
/// doc's scope/deferred bullets — non-literal path, non-`as_view`/non-string-`include` second argument,
/// and an unreducible regex all skip the entry, never guessed).
fn match_route_element(
    elt: &Expr,
    fns: &DjangoFns,
    idx: &crate::LineIndex,
) -> Option<RouterMountEntry> {
    let Expr::Call(call) = elt else { return None };
    let kind = element_kind(&call.func, fns)?;
    let Expr::StringLiteral(path_lit) = call.arguments.find_positional(0)? else {
        return None; // non-literal path/regex — never guessed
    };
    let raw = path_lit.value.to_str();
    let arg1 = call.arguments.find_positional(1)?;

    // `include('<dotted.module>')` — a cross-file mount. Django's include takes a STRING module path.
    if let Some(dotted) = match_include(arg1, fns) {
        let prefix = convert(kind, raw)?;
        // `ident` is the FULL dotted path (not the shared bare `urls` last segment): it equals the child
        // urlconf's fragment name (`rel_to_module_path`), so root-exclusion-by-name matches the child
        // even on a resolve miss, and it is unique across the composer's flat ident set.
        return Some(RouterMountEntry::Mount {
            prefix,
            ident: dotted.clone(),
            specifier: Some(dotted),
            attr_keys: Vec::new(),
        });
    }

    // `<View>.as_view()` — a concrete (verb-unknown) route. Any other second-argument shape (a bare
    // function-view reference, `include(router.urls)`, ...) is out of scope and skipped.
    if let Some(view) = match_as_view(arg1) {
        let path = convert(kind, raw)?;
        return Some(RouterMountEntry::Verb {
            method: UNKNOWN_VERB.to_string(),
            path,
            handler: Some(view),
            line: idx.line_of(call.range.start()),
            attr_keys: Vec::new(),
        });
    }
    None
}

fn convert(kind: Kind, raw: &str) -> Option<String> {
    match kind {
        Kind::Regex => regex_to_key(raw),
        Kind::Path => path_to_key(raw),
    }
}

/// Classify a `url`/`re_path`/`path` callee by its terminal name, resolved through the import alias map
/// (`from django.urls import path as p` binds `p` -> `path`). A terminal name absent from the map is
/// taken verbatim — covers the dotted `django.urls.path(...)` call form under a passed import gate.
fn element_kind(func: &Expr, fns: &DjangoFns) -> Option<Kind> {
    let name = callee_terminal_name(func)?;
    match original_of(name, fns) {
        "url" | "re_path" => Some(Kind::Regex),
        "path" => Some(Kind::Path),
        _ => None,
    }
}

/// `include('<dotted.module>')` -> the dotted module string, or `None` for a non-`include` callee OR a
/// non-string first argument (`include(router.urls)` — a DRF mount, deferred v1).
fn match_include(arg: &Expr, fns: &DjangoFns) -> Option<String> {
    let Expr::Call(call) = arg else { return None };
    let name = callee_terminal_name(&call.func)?;
    if original_of(name, fns) != "include" {
        return None;
    }
    let Expr::StringLiteral(s) = call.arguments.find_positional(0)? else {
        return None; // include(router.urls) / include((patterns, app)) — not a string module path
    };
    Some(s.value.to_str().to_string())
}

/// `<View>.as_view()` -> the view name (the receiver's terminal segment: `views.Home.as_view()` ->
/// `Home`). `None` for any other second-argument shape.
fn match_as_view(arg: &Expr) -> Option<String> {
    let Expr::Call(call) = arg else { return None };
    let Expr::Attribute(attr) = &*call.func else {
        return None;
    };
    if attr.attr.as_str() != "as_view" {
        return None;
    }
    match &*attr.value {
        Expr::Name(n) => Some(n.id.as_str().to_string()),
        Expr::Attribute(a) => Some(a.attr.as_str().to_string()),
        _ => None,
    }
}

/// The terminal name of a call callee (`url` in both `url(...)` and `django.urls.url(...)`).
fn callee_terminal_name(func: &Expr) -> Option<&str> {
    match func {
        Expr::Name(n) => Some(n.id.as_str()),
        Expr::Attribute(a) => Some(a.attr.as_str()),
        _ => None,
    }
}

fn original_of<'a>(name: &'a str, fns: &'a DjangoFns) -> &'a str {
    fns.get(name).map(String::as_str).unwrap_or(name)
}

mod convert_path;
use convert_path::{path_to_key, regex_to_key};

#[cfg(test)]
mod tests;
