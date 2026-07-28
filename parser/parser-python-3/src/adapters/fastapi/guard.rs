//! FastAPI `Depends(...)` AUTH-GUARD evidence — the Python half of the framework-neutral
//! decorator/annotation auth channel (`zzop_rules_http::mutating_route_no_auth`'s "Decorator/annotation
//! auth exemption": a `(file, line)` side-channel the engine mints into the `auth-guarded` attribute,
//! `zzop_rules_http::mutating_route_no_auth::AUTH_GUARDED_ATTR`). The Java sibling is
//! `zzop_parser_java_21::extract_spring_guarded_lines`; the TypeScript sibling is
//! `zzop_parser_typescript::extract_controller_guarded_lines`.
//!
//! ## Why this producer must exist for the call-graph lift to be honest
//! A FastAPI guard is applied by DEPENDENCY INJECTION, never by a call the handler body makes — so it is
//! structurally invisible to the call-graph BFS (exactly like a Spring `@PreAuthorize` or a Nest
//! `@UseGuards`). Adding `.py` to the BFS's covered-extension set WITHOUT this producer would turn every
//! `Depends`-guarded mutating route into a false positive; shipping this producer WITHOUT the BFS lift
//! would leave every Python route exempt and the rule silent. The two only make sense together.
//!
//! ## Recognized shapes (all from the real corpus checkouts named in `tests.rs`)
//! Import-gated on `fastapi` and restricted to TOP-LEVEL `def`/`async def` carrying a
//! `@<receiver>.<verb>(...)` route decorator whose receiver is a recognized FastAPI receiver — the exact
//! gate `super::extract_fastapi_router_fragments` uses, so an emitted line always coincides with a route
//! provide's own anchor line.
//! 1. **Decorator dependency list** — `@router.put("/{slug}", dependencies=[Depends(check_..._permissions)])`.
//! 2. **Parameter default** — `user: User = Depends(get_current_user_authorizer())`.
//! 3. **In-file `Annotated` parameter** — `user: Annotated[User, Depends(get_current_user)]`.
//! 4. **Tree-resolved `Annotated` ALIAS** — `current_user: CurrentUser`, where some file in the tree
//!    declares `CurrentUser = Annotated[User, Depends(get_current_user)]`
//!    ([`extract_python_guard_aliases`], collected tree-wide by the engine — which drops any name two
//!    files declare differently — and passed back in as the resolved guard set).
//!
//! ## Shape 4 is BOUND-name resolution, and the binding is checked in THIS file
//! A bare annotation is judged only when the name is actually bound here to the declaration the tree-wide
//! set is talking about: either this file DECLARES it as an alias (its own verdict then wins outright —
//! a local declaration shadows the tree), or this file IMPORTS the name. A name that is neither — an
//! unrelated local `class CurrentUser(BaseModel)` in some other module, which is a plain pydantic model —
//! is never judged, so its route keeps firing. Without that check the "resolution" was a bare-name match
//! against a tree-wide union, and the union was MONOTONE toward suppression: any one file declaring a
//! guard alias cleared every same-named annotation in the tree.
//!
//! Every shape bottoms out in [`depends`]'s single-expression judgment: the shared
//! [`crate::adapters::guard_vocab::is_guard_name`] vocabulary over the injected callable's name (see that
//! module for why it is precision-first), plus, for a FACTORY call, the call's own anonymous-permitting
//! switch (`get_current_user_authorizer(required=False)` names the same callee and means the opposite).
//!
//! ## Not recognized (honest under-recognition — the finding still fires)
//! A router-level `APIRouter(dependencies=[...])`, `fastapi.Security(...)`, a guard applied by a custom
//! decorator, and a `Depends` argument that is not a plain name/call (a lambda, a subscript).

use std::collections::{BTreeMap, BTreeSet};

use ruff_python_ast::{Expr, ExprCall, Stmt, StmtFunctionDef};
use zzop_core::ImportMap;

/// Every top-level SUBSCRIPT type alias (`X = Annotated[...]`, `X = Optional[User]`) declared in `text`,
/// each with the verdict "its `Annotated` list injects a guard-named `Depends`" — shape 4 of the module
/// doc's list. Import-gated on `fastapi` (the `Depends` marker is FastAPI's), and only a single
/// bare-`Name` assignment target counts (the same "simple single target only" discipline
/// `lang::symbols::const_symbol` applies). Empty on parse failure. Declaration order preserved.
///
/// NON-guard aliases are reported too, `false`-verdicted — the same shape the Django producer's
/// per-class verdicts have, and for the same reason: the engine joins these BY NAME across the tree, so
/// it needs to see a same-named DISAGREEMENT to drop it. Reporting only the guards would make the
/// tree-wide set monotone toward suppression.
///
/// The result is a TREE-WIDE input to [`extract_fastapi_guarded_lines`]: the alias is conventionally
/// declared in a shared `deps.py` and imported by every route module, so a per-file view would never see
/// the two halves together.
pub fn extract_python_guard_aliases(text: &str) -> Vec<(String, bool)> {
    extract_python_guard_aliases_with_vocab(
        text,
        &crate::adapters::guard_vocab::PythonGuardVocab::built_in(),
    )
}

/// [`extract_python_guard_aliases`] with the run's DECLARED Python guard vocabulary.
pub fn extract_python_guard_aliases_with_vocab(
    text: &str,
    vocab: &crate::adapters::guard_vocab::PythonGuardVocab<'_>,
) -> Vec<(String, bool)> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    if !super::imports_fastapi(&imports) {
        return Vec::new();
    }
    let bound = bound_dependency_verdicts(&module, &imports);
    alias_verdicts(&module, &imports, &bound, vocab)
}

/// [`extract_python_guard_aliases`]'s body, reused by [`extract_fastapi_guarded_lines`] for its OWN file
/// (the local-declaration half of the shape-4 binding check) off the module it already parsed.
fn alias_verdicts(
    module: &ruff_python_ast::ModModule,
    imports: &ImportMap,
    bound: &BoundVerdicts,
    vocab: &crate::adapters::guard_vocab::PythonGuardVocab<'_>,
) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for stmt in &module.body {
        let (target, value) = match stmt {
            Stmt::Assign(a) if a.targets.len() == 1 => (&a.targets[0], &*a.value),
            Stmt::AnnAssign(a) => match a.value.as_deref() {
                Some(v) => (&*a.target, v),
                None => continue,
            },
            _ => continue,
        };
        let (Expr::Name(name), Expr::Subscript(_)) = (target, value) else {
            continue;
        };
        out.push((
            name.id.as_str().to_string(),
            annotated_carries_guard_depends(value, imports, bound, vocab),
        ));
    }
    out
}

/// Route-registration lines in `text` that carry auth-guard evidence — module doc. `guard_aliases` is the
/// engine's RESOLVED tree-wide guard-alias set (same-name disagreements already dropped). Lines are
/// ascending and deduped; empty on parse failure and whenever the file does not import `fastapi` (never
/// panics).
pub fn extract_fastapi_guarded_lines(
    rel: &str,
    text: &str,
    guard_aliases: &BTreeSet<String>,
) -> Vec<u32> {
    extract_fastapi_guarded_lines_with_vocab(
        rel,
        text,
        guard_aliases,
        &crate::adapters::guard_vocab::PythonGuardVocab::built_in(),
    )
}

/// [`extract_fastapi_guarded_lines`] with the run's DECLARED Python guard vocabulary.
pub fn extract_fastapi_guarded_lines_with_vocab(
    _rel: &str,
    text: &str,
    guard_aliases: &BTreeSet<String>,
    vocab: &crate::adapters::guard_vocab::PythonGuardVocab<'_>,
) -> Vec<u32> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    if !super::imports_fastapi(&imports) {
        return Vec::new();
    }
    let receivers = super::route_receiver_names(&module);
    if receivers.is_empty() {
        return Vec::new();
    }
    let idx = crate::LineIndex::new(text);
    let bound = bound_dependency_verdicts(&module, &imports);
    let local_aliases: BTreeMap<String, bool> = alias_verdicts(&module, &imports, &bound, vocab)
        .into_iter()
        .collect();

    let mut out: Vec<u32> = Vec::new();
    for stmt in &module.body {
        let Stmt::FunctionDef(f) = stmt else { continue };
        // Signature-level evidence covers EVERY route decorator on this function (shapes 2-4); a
        // decorator's own `dependencies=` list (shape 1) covers only that decorator.
        let signature_guarded =
            signature_carries_guard(f, &imports, guard_aliases, &local_aliases, &bound, vocab);
        for dec in &f.decorator_list {
            let Expr::Call(call) = &dec.expression else {
                continue;
            };
            if !is_route_decorator(call, &receivers) {
                continue;
            }
            if signature_guarded || decorator_dependencies_guarded(call, &imports, &bound, vocab) {
                out.push(idx.line_of(dec.range.start()));
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// `@<receiver>.<verb>(...)` where `<receiver>` is a recognized FastAPI receiver and the decorator
/// actually mints a provide. BOTH of the conditions `super::collect_verb_entries` imposes on top of the
/// receiver gate are asked here through the SAME functions it calls, never through a parallel copy —
/// that sharing is what makes "a returned line always coincides with a provide's anchor" true rather
/// than merely intended:
/// - at least one HTTP method (`super::decorator_methods`) — a bare `@router.api_route("/x")` with no
///   literal `methods=` mints no provide;
/// - a literal path (`super::decorator_path_literal`) — `@router.post(ROOT)` mints no provide either.
///
/// A guard line for a decorator that mints no provide would anchor on nothing — or, if a sibling
/// decorator on the same function did mint one, on the wrong one.
fn is_route_decorator(call: &ExprCall, receivers: &BTreeSet<String>) -> bool {
    let Expr::Attribute(attr) = &*call.func else {
        return false;
    };
    let Expr::Name(recv) = &*attr.value else {
        return false;
    };
    if !receivers.contains(recv.id.as_str()) {
        return false;
    }
    !super::decorator_methods(attr.attr.as_str(), call).is_empty()
        && super::decorator_path_literal(call).is_some()
}

/// Shape 1 — a route decorator's `dependencies=[Depends(<guard>), ...]` keyword.
fn decorator_dependencies_guarded(
    call: &ExprCall,
    imports: &ImportMap,
    bound: &BoundVerdicts,
    vocab: &crate::adapters::guard_vocab::PythonGuardVocab<'_>,
) -> bool {
    let Some(kw) = call.arguments.find_keyword("dependencies") else {
        return false;
    };
    let elements: &[Expr] = match &kw.value {
        Expr::List(l) => &l.elts,
        Expr::Tuple(t) => &t.elts,
        _ => return false,
    };
    elements
        .iter()
        .any(|e| depends_names_a_guard(e, imports, bound, vocab))
}

/// Shapes 2-4 — the route function's own parameter list. `local_aliases` is THIS file's own alias
/// verdicts (module doc, "Shape 4 is BOUND-name resolution").
fn signature_carries_guard(
    f: &StmtFunctionDef,
    imports: &ImportMap,
    guard_aliases: &BTreeSet<String>,
    local_aliases: &BTreeMap<String, bool>,
    bound: &BoundVerdicts,
    vocab: &crate::adapters::guard_vocab::PythonGuardVocab<'_>,
) -> bool {
    for p in f.parameters.iter() {
        if let ruff_python_ast::AnyParameterRef::NonVariadic(with_default) = p {
            // Shape 2 — `= Depends(<guard>)`.
            if let Some(default) = with_default.default.as_deref() {
                if depends_names_a_guard(default, imports, bound, vocab) {
                    return true;
                }
            }
        }
        let Some(annotation) = p.as_parameter().annotation.as_deref() else {
            continue;
        };
        // Shape 3 — `Annotated[..., Depends(<guard>)]` written inline.
        if annotated_carries_guard_depends(annotation, imports, bound, vocab) {
            return true;
        }
        // Shape 4 — a bare annotation naming a guard alias BOUND IN THIS FILE (module doc). A local
        // declaration answers on its own (shadowing the tree); otherwise the name must be imported here
        // AND carry the resolved tree-wide guard verdict. Neither => never judged.
        if let Expr::Name(n) = annotation {
            let name = n.id.as_str();
            let judged = match local_aliases.get(name) {
                Some(local) => *local,
                None => imports.contains_key(name) && guard_aliases.contains(name),
            };
            if judged {
                return true;
            }
        }
    }
    false
}

mod depends;

use depends::{
    annotated_carries_guard_depends, bound_dependency_verdicts, depends_names_a_guard,
    BoundVerdicts,
};

#[cfg(test)]
mod tests;
