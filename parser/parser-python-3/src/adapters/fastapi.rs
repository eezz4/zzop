//! FastAPI route PROVIDES, projected as framework-neutral router-mount fragments (the same shape
//! `zzop_parser_typescript::adapters::router_mounts` emits) — composed into whole-tree `IoProvide`s by
//! the engine's assemble pass. See that module's doc for the fragment shape rationale (a router split
//! across mounts/prefixes can't be resolved to a real URL from one file alone).
//!
//! ## Scope (v1)
//! Import-gated on `fastapi` (any specifier `"fastapi"` or `"fastapi.<...>"`) — a file that never imports
//! FastAPI yields no fragments, never a bare-name guess. Recognition is restricted to TOP-LEVEL
//! statements only (`module.body`'s direct children), mirroring `lang::symbols`'s "top-level only" v1
//! scope: a receiver assignment, a route decorator on a nested `def`, or an `include_router` call inside
//! a function/conditional/class body are all out of scope for v1 (documented limitation — covers the
//! overwhelming majority of real FastAPI apps, which register routes at module level).
//!
//! - **Receivers**: `x = FastAPI(...)` and `x = APIRouter(...)` (bare-name OR `fastapi.FastAPI(...)`-
//!   qualified callee, single bare-`Name` assignment target only). An `APIRouter(prefix="/p")`
//!   STRING-LITERAL `prefix` kwarg is captured and prepended to that receiver's own verb paths at
//!   emission (visible-literal pre-composition, not re-normalized here — the engine's key builder
//!   handles duplicate slashes downstream); a NON-literal `prefix` kwarg vetoes every verb entry for
//!   that receiver (never guessed).
//! - **Verbs**: `@<receiver>.get|post|put|patch|delete("<path>")` decorating a top-level `def`/`async def`
//!   -> `RouterMountEntry::Verb{method: UPPERCASE, path, handler: Some(fn name), line: decorator line,
//!   attr_keys: vec![]}`. A non-literal path argument, or a decorator naming a verb this crate doesn't
//!   recognize, skips just that decorator (the function may still carry other qualifying decorators, and
//!   is still itself a `SourceSymbol` via `lang::symbols` regardless).
//! - **Mounts**: `<receiver>.include_router(<ident>, prefix="/api")` -> `RouterMountEntry::Mount{prefix:
//!   <literal or "/">, ident, specifier: <this file's ImportMap specifier for `ident`, else None>,
//!   attr_keys: vec![]}`. A non-`Name` first argument, or a non-literal `prefix` kwarg, skips the mount
//!   entirely (subtree silence is honest — never a guessed prefix).
//! - One `RouterMountFragment` per receiver with at least one surviving entry, in first-appearance order;
//!   a receiver with zero surviving entries produces no fragment (mirrors
//!   `zzop_parser_typescript::adapters::router_mounts`'s same rule).

use std::collections::HashMap;

use ruff_python_ast::{Expr, Stmt, StmtExpr, StmtFunctionDef};
use zzop_core::{ImportMap, RouterMountEntry, RouterMountFragment};

/// FastAPI's own HTTP-method decorator names — lowercase, UPPERCASE-normalized at emission. A dedicated
/// vocabulary (not `zzop_core::HTTP_KEY_VERBS`, which is uppercase-spelled): this one names DECORATORS,
/// that one names KEY-BUILDING verbs; they happen to agree one-for-one for these five.
pub(crate) const VERB_DECORATORS: &[&str] = &["get", "post", "put", "patch", "delete"];

struct ReceiverInfo {
    /// `Some(prefix)` when an `APIRouter(prefix="literal")` kwarg was captured.
    prefix: Option<String>,
    /// `true` when `prefix=` was present but NOT a string literal — vetoes every verb entry for this
    /// receiver (never guessed).
    skip_verbs: bool,
}

/// Extract this file's FastAPI router-mount fragments — see module doc. `rel` is accepted for public-API
/// parity with this crate's other extractors but unused: a `RouterMountFragment` carries no `file` field
/// of its own (each entry's `line` is anchored within THIS file, and the engine composes/keys fragments
/// without needing the source path back). Returns an empty vec on parse failure, and whenever the file
/// does not import `fastapi` (never panics).
pub fn extract_fastapi_router_fragments(_rel: &str, text: &str) -> Vec<RouterMountFragment> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    if !imports_fastapi(&imports) {
        return Vec::new();
    }
    let idx = crate::LineIndex::new(text);

    let mut receiver_order: Vec<String> = Vec::new();
    let mut receivers: HashMap<String, ReceiverInfo> = HashMap::new();
    for stmt in &module.body {
        if let Some((name, info)) = match_receiver(stmt) {
            if !receivers.contains_key(&name) {
                receiver_order.push(name.clone());
            }
            receivers.insert(name, info);
        }
    }
    if receivers.is_empty() {
        return Vec::new();
    }

    let mut entries: HashMap<String, Vec<RouterMountEntry>> = HashMap::new();
    for stmt in &module.body {
        match stmt {
            Stmt::FunctionDef(f) => collect_verb_entries(f, &receivers, &idx, &mut entries),
            Stmt::Expr(e) => {
                if let Some((recv, entry)) = match_include_router(e, &receivers, &imports) {
                    entries.entry(recv).or_default().push(entry);
                }
            }
            _ => {}
        }
    }

    receiver_order
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

fn imports_fastapi(imports: &ImportMap) -> bool {
    imports
        .values()
        .any(|b| b.specifier == "fastapi" || b.specifier.starts_with("fastapi."))
}

/// `x = FastAPI(...)` / `x = APIRouter(...)` (bare-name or `<module>.FastAPI(...)`-qualified callee,
/// single bare-`Name` target only) -> `(receiver name, ReceiverInfo)`.
fn match_receiver(stmt: &Stmt) -> Option<(String, ReceiverInfo)> {
    let Stmt::Assign(a) = stmt else { return None };
    if a.targets.len() != 1 {
        return None;
    }
    let Expr::Name(target) = &a.targets[0] else {
        return None;
    };
    let Expr::Call(call) = &*a.value else {
        return None;
    };
    let callee = callee_name(&call.func)?;
    match callee {
        "FastAPI" => Some((
            target.id.as_str().to_string(),
            ReceiverInfo {
                prefix: None,
                skip_verbs: false,
            },
        )),
        "APIRouter" => {
            let (prefix, skip_verbs) = match call.arguments.find_keyword("prefix") {
                Some(kw) => match &kw.value {
                    Expr::StringLiteral(s) => (Some(s.value.to_str().to_string()), false),
                    _ => (None, true),
                },
                None => (None, false),
            };
            Some((
                target.id.as_str().to_string(),
                ReceiverInfo { prefix, skip_verbs },
            ))
        }
        _ => None,
    }
}

/// The bare or dotted-qualified callee name of a call expression (`FastAPI` in both `FastAPI(...)` and
/// `fastapi.FastAPI(...)`).
fn callee_name(func: &Expr) -> Option<&str> {
    match func {
        Expr::Name(n) => Some(n.id.as_str()),
        Expr::Attribute(a) => Some(a.attr.as_str()),
        _ => None,
    }
}

/// Every verb-decorated route on `f`, keyed by receiver — appended into `entries`.
fn collect_verb_entries(
    f: &StmtFunctionDef,
    receivers: &HashMap<String, ReceiverInfo>,
    idx: &crate::LineIndex,
    entries: &mut HashMap<String, Vec<RouterMountEntry>>,
) {
    for dec in &f.decorator_list {
        let Expr::Call(call) = &dec.expression else {
            continue;
        };
        let Expr::Attribute(attr) = &*call.func else {
            continue;
        };
        let Expr::Name(recv) = &*attr.value else {
            continue;
        };
        let receiver_name = recv.id.as_str();
        let Some(info) = receivers.get(receiver_name) else {
            continue;
        };
        if info.skip_verbs {
            continue;
        }
        let verb = attr.attr.as_str();
        if !VERB_DECORATORS.contains(&verb) {
            continue;
        }
        let Some(Expr::StringLiteral(path_lit)) = call.arguments.find_positional(0) else {
            continue;
        };
        let path = match &info.prefix {
            Some(prefix) => format!("{prefix}{}", path_lit.value.to_str()),
            None => path_lit.value.to_str().to_string(),
        };
        entries
            .entry(receiver_name.to_string())
            .or_default()
            .push(RouterMountEntry::Verb {
                method: verb.to_ascii_uppercase(),
                path,
                handler: Some(f.name.to_string()),
                line: idx.line_of(dec.range.start()),
                attr_keys: Vec::new(),
            });
    }
}

/// `<receiver>.include_router(<ident>, prefix="...")` -> `Mount`, or `None` for any non-qualifying shape
/// (see module doc's "Mounts" bullet for the exact skip rules).
fn match_include_router(
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
    let Some(Expr::Name(router_ident)) = call.arguments.find_positional(0) else {
        return None; // non-identifier first argument — never guessed
    };
    let ident = router_ident.id.as_str().to_string();
    let prefix = match call.arguments.find_keyword("prefix") {
        Some(kw) => match &kw.value {
            Expr::StringLiteral(s) => s.value.to_str().to_string(),
            _ => return None, // non-literal prefix — skip the mount entirely
        },
        None => "/".to_string(),
    };
    let specifier = imports.get(&ident).map(|b| b.specifier.clone());
    Some((
        receiver_name.to_string(),
        RouterMountEntry::Mount {
            prefix,
            ident,
            specifier,
            attr_keys: Vec::new(),
        },
    ))
}

#[cfg(test)]
mod tests;
