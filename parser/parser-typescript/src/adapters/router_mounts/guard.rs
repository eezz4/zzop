//! Express middleware guard-name judgment for `router_mounts`. Feeds `build::FragmentBuilder`'s
//! `.use`/route-level middle-arg classification: a recognized guard name/callee mints a
//! `RouterMountEntry::ScopedAttr`/`attr_keys` entry (`AUTH_GUARDED_ATTR_KEY`) rather than being
//! silently dropped as unresolvable middleware. See the parent module doc for the recognizer
//! spec this feeds into.

use swc_core::ecma::ast::{Callee, Expr};

use super::build::handler_name;
use super::chain::unwrap_expr;

/// Well-known auth middleware factory callees (dotted chain text) judged guard-certain regardless of
/// the name pattern.
const MIDDLEWARE_GUARD_CALLEES: &[&str] = &[
    "passport.authenticate",
    "expressjwt",
    "requiresAuth",
    "clerkMiddleware",
    "ensureLoggedIn",
    "checkJwt",
];
/// Identifier-tail suffixes that VETO the guard-name judgment outright, checked BEFORE the guard
/// predicate below. `authController`/`authService`/`authApi`/`authModule`/`authRouter`/`authRoutes`
/// are sub-router or DI shapes, not middleware guards: when such a mount fails to resolve (the
/// common case — these are almost never re-exported under a name this recognizer's fragment index
/// can find), the OLD code fell through to the guard-name check anyway and emitted a false
/// `PathScope` guard attribute, silently suppressing a real `mutating-route-no-auth` finding under
/// that whole subtree. A false guard attribute is strictly worse than a missed one here (see the
/// precision-first rationale below), so this veto runs unconditionally, first.
const ROUTER_NAME_VETO_SUFFIXES: &[&str] = &[
    "router",
    "routes",
    "route",
    "controller",
    "service",
    "module",
    "client",
    "store",
    "config",
    "api",
];
/// The attribute key emitted for judged guards. Producer<->consumer contract vocabulary — pairs with
/// rules-http's `AUTH_GUARDED_ATTR` (`"auth-guarded"`); the engine e2e pins the pairing.
pub(super) const AUTH_GUARDED_ATTR_KEY: &str = "auth-guarded";

/// Judges whether a lowercased identifier TAIL (the name after the last `.`, already lowercased by
/// the caller) is a middleware guard name. INTENTIONALLY NARROWER than rules-http's
/// `DEFAULT_AUTH_GUARD_PATTERN` (the BFS judgment vocab): an attribute emitted from this judgment
/// SUPPRESSES a `mutating-route-no-auth` finding, so precision beats recall here — a false positive
/// is a silent, undiscoverable miss on the consumer side, worse than the recall this drops.
///
/// Excluded on purpose (never judged a guard):
/// - `session` (express-session adds session STATE, it does not reject requests).
/// - `admin|owner|role|is(local|dev|production)` (too broad for a middleware NAME judgment alone).
/// - Bare `verify` (traps like `verifyContentLength`/`verifyEmail`) and bare substring `token`
///   (traps like `tokenizer`/`tokenBucket`, a rate limiter). `verifyToken`/`verifyJwt`/`checkToken`
///   still match via the `auth`/`jwt`/`token` SUFFIX rules below — only the bare, unanchored forms
///   are dropped.
///
/// Rust's `regex` crate has no lookaround, so this is an explicit predicate (not a single regex) —
/// the `auth`-but-not-`author` rule in particular has no clean regex encoding without it.
fn is_guard_name(tail: &str) -> bool {
    if tail.contains("authoriz") || tail.contains("authentic") {
        return true;
    }
    // Lookaround workaround: `author*` names (`author`, `authorId`) must not match bare `auth` —
    // `authorize`/`authorization`/`authentic*` are already caught above.
    if tail.contains("auth") && !tail.contains("author") {
        return true;
    }
    if tail.ends_with("guard")
        || tail.ends_with("jwt")
        || tail.ends_with("token")
        || tail.ends_with("apikey")
    {
        return true;
    }
    if tail.contains("permission") || tail.contains("loggedin") {
        return true;
    }
    if tail.ends_with("acl") {
        return true;
    }
    if tail.contains("hasaccess")
        || tail.contains("canaccess")
        || tail.contains("checkaccess")
        || tail.contains("requireaccess")
    {
        return true;
    }
    false
}

/// Judges whether an argument expression is a middleware guard, per this module's guard vocabulary
/// (see [`is_guard_name`]/`MIDDLEWARE_GUARD_CALLEES`/`ROUTER_NAME_VETO_SUFFIXES` docs above). Only
/// visible ident/dotted-member/call shapes are ever judged — anything else (arrow functions, arrays,
/// object literals) is never guessed, returning `false`.
///
/// - A bare `Ident` or dotted member chain (`auth.optional`): judged via its TAIL name (after the
///   last `.`, lowercased) against [`is_guard_name`], vetoed first by [`ROUTER_NAME_VETO_SUFFIXES`].
/// - A `Call` (`requireAuth()`, `passport.authenticate('jwt')`): the callee's dotted text is checked
///   against [`MIDDLEWARE_GUARD_CALLEES`] first (guard-certain, no veto applies); otherwise the
///   callee's TAIL name is judged the same way a bare ident/member argument is.
///
/// `e` is expected to already be unwrapped (see `unwrap_expr`) by the caller.
pub(super) fn judge_guard_arg(e: &Expr) -> bool {
    let is_guard_tail = |dotted: &str| -> bool {
        let tail = dotted
            .rsplit('.')
            .next()
            .unwrap_or(dotted)
            .to_ascii_lowercase();
        if ROUTER_NAME_VETO_SUFFIXES.iter().any(|s| tail.ends_with(s)) {
            return false;
        }
        is_guard_name(&tail)
    };

    match e {
        Expr::Call(call) => {
            let Callee::Expr(callee) = &call.callee else {
                return false;
            };
            let Some(dotted) = handler_name(unwrap_expr(callee)) else {
                return false;
            };
            if MIDDLEWARE_GUARD_CALLEES.contains(&dotted.as_str()) {
                return true;
            }
            is_guard_tail(&dotted)
        }
        Expr::Ident(_) | Expr::Member(_) => {
            handler_name(e).is_some_and(|dotted| is_guard_tail(&dotted))
        }
        _ => false,
    }
}
