//! `include_router` mount matching for `adapters::fastapi` — extracted from `fastapi.rs` (file-size
//! limit). See the parent module doc's "Mounts" bullet for the two child shapes (bare name / module
//! attribute) and the base-name-ident rationale.

use std::collections::HashMap;

use ruff_python_ast::{Expr, StmtExpr};
use zzop_core::{ImportMap, RouterMountEntry};

use super::ReceiverInfo;

/// A dotted attribute access spelled back out (`settings.API_V1_STR`), or `None` for any other shape.
///
/// Only dotted accesses are carried, matching the const map's own dotted-keys-only rule: a BARE name
/// (`prefix=API_PREFIX`) is deliberately not emitted as a ref, because the map never holds bare names —
/// a project-wide scope-insensitive lookup on `prefix` or `base` would resolve someone else's local.
/// A bare-name prefix therefore stays unread, and S14 keeps reporting it.
fn dotted_ref(expr: &Expr) -> Option<String> {
    // The TOP node must be an attribute access — a bare `Expr::Name` is rejected here even though the
    // recursion below accepts one as the BASE of a dotted chain (`settings` in `settings.API_V1_STR`).
    let Expr::Attribute(_) = expr else {
        return None;
    };
    fn walk(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Attribute(attr) => Some(format!("{}.{}", walk(&attr.value)?, attr.attr)),
            Expr::Name(name) => Some(name.id.to_string()),
            _ => None,
        }
    }
    walk(expr)
}

/// `<receiver>.include_router(<child>, prefix="...")` -> `Mount`, a dotted non-literal prefix ->
/// `MountRef`, or `None` for any non-qualifying shape (see the parent module doc's "Mounts" bullet).
pub(super) fn match_include_router(
    stmt: &StmtExpr,
    receivers: &HashMap<String, ReceiverInfo>,
    imports: &ImportMap,
) -> Option<(String, RouterMountEntry)> {
    let Expr::Call(call) = &*stmt.value else {
        return None;
    };
    let Expr::Attribute(attr) = &*call.func else {
        return None;
    };
    if attr.attr.as_str() != "include_router" {
        return None;
    }
    let Expr::Name(recv) = &*attr.value else {
        return None;
    };
    let receiver_name = recv.id.as_str();
    if !receivers.contains_key(receiver_name) {
        return None;
    }
    let (ident, specifier) = match call.arguments.find_positional(0)? {
        // `include_router(auth_router, ...)` — a bare imported/local router name. Specifier (if the name
        // is imported) points at the module the router came from; a locally-defined router leaves it None
        // so the engine resolves same-file.
        Expr::Name(router_ident) => {
            let ident = router_ident.id.as_str().to_string();
            let specifier = imports.get(&ident).map(|b| b.specifier.clone());
            (ident, specifier)
        }
        // `include_router(authentication.router, ...)` — the canonical `import <mod>; <mod>.router` form.
        // Reconstruct module `<mod>`'s full dotted path from the base name's import binding (specifier +
        // "." + original — e.g. `from app.api.routes import authentication` → `app.api.routes` +
        // `authentication`) as the specifier, so the engine resolves it to that module's file and picks up
        // its SOLE router fragment. `ident` is the BASE module name, NOT the `.router` attribute: every
        // FastAPI router is conventionally named `router`, so using `router` as the mount ident would
        // poison the composition's root-exclusion-by-name — a single mount targeting `router` disqualifies
        // EVERY `router`-named fragment (including an un-mounted top-of-chain router whose own app-level
        // mount was skipped for a non-literal prefix) from being a DFS root, collapsing the whole tree to
        // zero provides. The per-module base name is distinct, so it excludes only the intended child.
        // An attribute whose base is not a known import is not guessed.
        Expr::Attribute(attr_expr) => {
            let Expr::Name(base) = &*attr_expr.value else {
                return None;
            };
            let binding = imports.get(base.id.as_str())?;
            let ident = base.id.as_str().to_string();
            let specifier = format!("{}.{}", binding.specifier, binding.original);
            (ident, Some(specifier))
        }
        _ => return None, // any other first-argument shape — never guessed
    };
    // A non-literal prefix is not skipped any more: it rides as a `MountRef` whose `prefix_ref` the
    // engine resolves against the project-wide const map, exactly as a `@Controller(RouteKey.X)`
    // prefix already did. Dropping it here used to be silent AND lossy — the child router's routes
    // were still emitted, just without the prefix, which is a WRONG key rather than a missing one.
    // Unresolvable refs are dropped and disclosed by the composer, never defaulted to `/`.
    let entry = match call.arguments.find_keyword("prefix") {
        Some(kw) => match &kw.value {
            Expr::StringLiteral(s) => RouterMountEntry::Mount {
                prefix: s.value.to_str().to_string(),
                ident,
                specifier,
                attr_keys: Vec::new(),
            },
            other => RouterMountEntry::MountRef {
                prefix_ref: dotted_ref(other)?,
                ident,
                specifier,
                attr_keys: Vec::new(),
            },
        },
        None => RouterMountEntry::Mount {
            prefix: "/".to_string(),
            ident,
            specifier,
            attr_keys: Vec::new(),
        },
    };
    Some((receiver_name.to_string(), entry))
}
