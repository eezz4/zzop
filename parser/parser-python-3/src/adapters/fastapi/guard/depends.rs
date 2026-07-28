//! What ONE `Depends(...)` expression means — the leaf judgment every shape in the parent module
//! (`adapters::fastapi::guard`) bottoms out in. Split out of `guard.rs` purely to stay under the repo's
//! per-file line cap; every item here is `pub(super)` or private.
//!
//! The judgment has two halves, and both must pass before a route is cleared: the injected callable's
//! NAME must read as a guard (`crate::adapters::guard_vocab::is_guard_name`), and — when the callable is
//! produced by a FACTORY CALL — the call's own arguments must not say it was configured to let anonymous
//! callers through. The second half exists because the first cannot see it:
//! `get_current_user_authorizer()` and `get_current_user_authorizer(required=False)` have the identical
//! callee name and the opposite meaning.
//!
//! ## Where the switch is READ is where it is WRITTEN — and absence of a binding is not "undecidable"
//! The anonymous switch is only ever consulted on a construction this file can SEE. It is written in one
//! of two places, and [`bound_dependency_verdicts`] is the second:
//! 1. AT the `Depends` site — `Depends(get_current_user_authorizer(required=False))`.
//! 2. At a top-level ASSIGNMENT this file makes — `oauth2_scheme = OAuth2PasswordBearer(tokenUrl=...)`,
//!    then `Depends(oauth2_scheme)`. The `Depends` site shows only a bare name, so without the
//!    assignment the switch would be invisible; the judgment itself is the same one case 1 makes.
//!
//! A name with NO visible construction (`Depends(get_current_user)`, an import from another module) is a
//! third case and is deliberately NOT treated as an undecidable switch. "Undecidable pays with recall"
//! governs a switch that is PRESENT but unreadable (`required=cfg.strict`) — positive evidence that this
//! particular construction may have been opted out. An unresolved name carries no such evidence: it is
//! simply outside the switch's domain, where the NAME vocabulary is the whole evidence base and always
//! was (module `guard`'s stated contract). Collapsing case 3 into case 2 would reject nearly every
//! `Depends` in the corpus — measured, that is the same state as deleting this producer (9 and 14 extra
//! `mutating-route-no-auth` findings on `be-fastapi` / `be-fastapi-fs`, all of them on guarded routes).

use std::collections::BTreeMap;

use ruff_python_ast::{Expr, ExprCall, ModModule, Stmt};
use zzop_core::ImportMap;

use crate::adapters::guard_vocab::is_guard_name;

/// `Annotated[<T>, Depends(<guard>), ...]` — true when any subscript element is a guard-naming
/// `Depends(...)` call. The `Annotated` head itself is not name-checked (it is a `typing` re-export with
/// many spellings); the `Depends` marker inside is what carries the meaning.
pub(super) fn annotated_carries_guard_depends(
    e: &Expr,
    imports: &ImportMap,
    bound: &BoundVerdicts,
    vocab: &crate::adapters::guard_vocab::PythonGuardVocab<'_>,
) -> bool {
    let Expr::Subscript(sub) = e else {
        return false;
    };
    match &*sub.slice {
        Expr::Tuple(t) => t
            .elts
            .iter()
            .any(|el| depends_names_a_guard(el, imports, bound, vocab)),
        other => depends_names_a_guard(other, imports, bound, vocab),
    }
}

/// `Depends(<callable>)` whose `<callable>`'s name reads as a guard. `<callable>` may be a bare name
/// (`Depends(get_current_user)`), a dotted member (`Depends(authentication.get_current_user_authorizer)`)
/// or a FACTORY call (`Depends(get_current_user_authorizer())`) — the corpus uses all three. Any other
/// argument shape (a lambda, a subscript) is never judged.
pub(super) fn depends_names_a_guard(
    e: &Expr,
    imports: &ImportMap,
    bound: &BoundVerdicts,
    vocab: &crate::adapters::guard_vocab::PythonGuardVocab<'_>,
) -> bool {
    let Expr::Call(call) = e else { return false };
    let Some(head) = callee_terminal_name(&call.func) else {
        return false;
    };
    if resolve_original(head, imports) != "Depends" {
        return false;
    }
    let Some(arg) = call.arguments.find_positional(0) else {
        return false;
    };
    match arg {
        // A bare name this file BINDS to a construction answers from that construction (module doc,
        // case 2); any other bare name falls through to the vocabulary (case 3).
        Expr::Name(n) => match bound.get(n.id.as_str()) {
            Some(verdict) => *verdict,
            None => is_guard_name(n.id.as_str(), vocab),
        },
        Expr::Attribute(a) => is_guard_name(a.attr.as_str(), vocab),
        // A factory call written AT the `Depends` site (module doc, case 1).
        Expr::Call(inner) => {
            !factory_permits_anonymous(inner)
                && callee_terminal_name(&inner.func).is_some_and(|n| is_guard_name(n, vocab))
        }
        // A lambda, a subscript — never judged, never guessed.
        _ => false,
    }
}

/// `name -> "a `Depends(name)` on this name rejects anonymous callers"`, for the names this file binds
/// to a call. Only names the binding DECIDES are present; everything else is absent and left to the name
/// vocabulary. See [`bound_dependency_verdicts`].
pub(super) type BoundVerdicts = BTreeMap<String, bool>;

/// fastapi's own security-scheme classes (`fastapi.security`). An instance of one of these IS the gate:
/// it reads the credential off the request and raises 401 when it is missing, unless the construction
/// passes `auto_error=False`. Recognizing the CONSTRUCTION is what lets this producer answer for names
/// the vocabulary reads wrongly in BOTH directions — `api_key_header = APIKeyHeader(...)` (which the
/// `header` noun-form veto would reject) and `oauth2_scheme = OAuth2PasswordBearer(..., auto_error=False)`
/// (which the bare-`auth` arm would accept).
const SECURITY_SCHEME_CLASSES: &[&str] = &[
    "APIKeyCookie",
    "APIKeyHeader",
    "APIKeyQuery",
    "HTTPBasic",
    "HTTPBearer",
    "HTTPDigest",
    "OAuth2",
    "OAuth2AuthorizationCodeBearer",
    "OAuth2PasswordBearer",
    "OpenIdConnect",
];

/// Module doc case 2 — every top-level `<name> = <call>(...)` in this file that DECIDES whether a
/// `Depends(<name>)` on it rejects anonymous callers. Two bindings decide, and nothing else is recorded:
/// - a [`SECURITY_SCHEME_CLASSES`] construction — a structural gate, verdict = "its switch is on". This
///   OVERRIDES the name vocabulary in both directions, because the construction is stronger evidence
///   than the spelling of the variable it was stored in.
/// - any other call whose anonymous switch is explicitly off (`x = build_auth(required=False)`) — the
///   same `Depends(factory(required=False))` judgment, written one statement earlier. Only the `false`
///   direction is recorded here: an ordinary factory call says nothing POSITIVE that the name did not
///   already say.
///
/// Same "simple single target only" discipline as `super::alias_verdicts`. A rebinding replaces the
/// earlier verdict (last top-level assignment wins), matching what the interpreter would do.
pub(super) fn bound_dependency_verdicts(module: &ModModule, imports: &ImportMap) -> BoundVerdicts {
    let mut out = BoundVerdicts::new();
    for stmt in &module.body {
        let (target, value) = match stmt {
            Stmt::Assign(a) if a.targets.len() == 1 => (&a.targets[0], &*a.value),
            Stmt::AnnAssign(a) => match a.value.as_deref() {
                Some(v) => (&*a.target, v),
                None => continue,
            },
            _ => continue,
        };
        let (Expr::Name(name), Expr::Call(call)) = (target, value) else {
            continue;
        };
        let rejects_anonymous = !factory_permits_anonymous(call);
        let is_scheme = callee_terminal_name(&call.func)
            .is_some_and(|head| SECURITY_SCHEME_CLASSES.contains(&resolve_original(head, imports)));
        if is_scheme {
            out.insert(name.id.as_str().to_string(), rejects_anonymous);
        } else if !rejects_anonymous {
            out.insert(name.id.as_str().to_string(), false);
        }
    }
    out
}

/// Keywords whose value decides, AT THE CALL SITE, whether a guard factory's product actually rejects an
/// anonymous caller. `required` is FastAPI-app vocabulary
/// (`corpus/oss/be-fastapi/app/api/dependencies/authentication.py`: `get_current_user_authorizer(*,
/// required: bool = True)` returns `_get_current_user` or `_get_current_user_optional`, and the latter
/// returns `None` for a caller with no token); `auto_error` is the same switch on fastapi's own security
/// schemes (`APIKeyHeader(..., auto_error=False)` returns `None` instead of raising). Both default to
/// `True`, so an ABSENT keyword leaves the name judgment untouched.
const ANONYMOUS_SWITCH_KEYWORDS: &[&str] = &["required", "auto_error"];

/// True when a construction call — written at the `Depends` site or at a binding
/// ([`bound_dependency_verdicts`]) — is configured to permit anonymous callers, the case the name
/// vocabulary structurally cannot see. A keyword present with a non-literal value
/// (`required=cfg.strict`) is UNDECIDABLE and therefore also treated as permitting: this producer
/// suppresses findings, so an unreadable switch must cost recall, never precision.
fn factory_permits_anonymous(call: &ExprCall) -> bool {
    ANONYMOUS_SWITCH_KEYWORDS.iter().any(|name| {
        call.arguments
            .find_keyword(name)
            .is_some_and(|kw| !matches!(&kw.value, Expr::BooleanLiteral(b) if b.value))
    })
}

/// The terminal name of a call callee (`Depends` in both `Depends(...)` and `fastapi.Depends(...)`).
fn callee_terminal_name(func: &Expr) -> Option<&str> {
    match func {
        Expr::Name(n) => Some(n.id.as_str()),
        Expr::Attribute(a) => Some(a.attr.as_str()),
        _ => None,
    }
}

/// Maps a locally-bound name back to its imported original (`from fastapi import Depends as D` binds
/// `D` -> `Depends`); an unbound name is taken verbatim, which covers the dotted `fastapi.Depends(...)`
/// call form under the module's own import gate.
fn resolve_original<'a>(local: &'a str, imports: &'a ImportMap) -> &'a str {
    imports
        .get(local)
        .map(|b| b.original.as_str())
        .unwrap_or(local)
}
