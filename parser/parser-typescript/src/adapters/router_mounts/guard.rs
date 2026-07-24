//! Express middleware guard-name judgment for `router_mounts`. Feeds `build::FragmentBuilder`'s
//! `.use`/route-level middle-arg classification: a recognized guard name/callee mints a
//! `RouterMountEntry::ScopedAttr`/`attr_keys` entry (`AUTH_GUARDED_ATTR_KEY`) rather than being
//! silently dropped as unresolvable middleware. See the parent module doc for the recognizer
//! spec this feeds into.
//!
//! Two judgments live here, sharing ONE vocabulary on purpose (a second, drifting guard-word list
//! is how a security recognizer silently rots): [`judge_guard_arg`] for the middleware POSITION
//! (`.use(x)`, a route's middle args) and [`judge_guard_wrapper_arg`] for the higher-order-function
//! POSITION (a route's LAST arg, `router.get(p, requireAdmin(handler))`). The wrapper judgment is
//! the middleware one plus a small position-justified widening — see its doc for why the position
//! earns it.

use swc_core::ecma::ast::{CallExpr, Callee, Expr};

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

/// Rejection-verb prefixes accepted ONLY in the higher-order-function wrapper position (see
/// [`judge_guard_wrapper_arg`]). Every one of these reads "reject the request unless ..." — the
/// shape of a gate, not of the async/error/observability wrappers (`asyncHandler`, `catchAsync`,
/// `wrapAsync`, `withErrorHandler`, `withLogging`) that make up nearly all other real handler
/// wrappers, none of which start with any of these. A match must be strictly LONGER than the
/// prefix: bare `require` is Node's CommonJS loader, and `router.get(p, require('./handler'))` must
/// never read as authorization evidence.
///
/// A pure narrowing ADVERB (`adminOnly(handler)`, `ownerOnly(handler)`) is deliberately NOT accepted
/// even though it is a real idiom: `*Only` composes with every axis, not just authorization, and it
/// caught `nodeEnvOnly(handler)` — an ENV gate — the moment it was tried. See the env veto below for
/// why that distinction is load-bearing; the missed `adminOnly` is an accepted under-recognition
/// (the finding still fires) and `requireAdmin`/`ensureAdmin` cover the same intent.
const WRAPPER_GUARD_PREFIXES: &[&str] = &["require", "ensure", "protect", "restrict"];
/// Env-axis vocabulary that VETOES the wrapper judgment. `requireProduction(handler)` /
/// `ensureLocal(handler)` are rejection verbs, so the prefix rule alone would accept them — but an
/// env check gates WHERE code runs, not WHO may call it. That axis belongs to `route-exposure`, and
/// `auth-gates`' own tests pin that an env gate must never clear a missing-auth finding, so letting
/// one mint `auth-guarded` would silently defeat a rule the repo explicitly decided about. Vetoing
/// costs only recall on names that mix both axes (`requireDeveloperRole`) — the safe direction.
const ENV_AXIS_VETO_SUBSTRINGS: &[&str] = &["env", "prod", "staging", "local", "dev", "debug"];

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
    match e {
        Expr::Call(call) => {
            let Some(dotted) = callee_dotted(call) else {
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

/// Judges a route registration's LAST argument — the handler slot — for the higher-order-function
/// guard idiom: `router.get('/admin/x', requireAdmin(handler))`, a guard applied by WRAPPING the
/// handler instead of by sitting in front of it as middleware. Only a `Call` is ever judged: a bare
/// ident/member last arg is the plain handler, and judging its name would resurrect exactly the
/// removed "keyword in a handler identifier clears the finding" over-clear (`adminHandlers.list`).
///
/// [`judge_guard_arg`]'s vocabulary answers first, so the two positions can never disagree about a
/// name. On top of it this position — and ONLY this position — also accepts
/// [`WRAPPER_GUARD_PREFIXES`], which is what lets `requireAdmin`/`ensureOwner` through even though
/// `admin`/`owner`/`role` are deliberately absent from the middleware name vocabulary. The position
/// earns the widening: a middleware ARGUMENT LIST is full of non-guards (loggers, body parsers, rate
/// limiters, CORS), whereas a callee that takes a route handler and returns a route handler is a
/// small, rejection-skewed family. The widening is SHAPE-gated (a rejection verb), never a bare
/// `admin`/`role` substring, so a factory like `createRoleHandler(deps)` still does NOT clear its
/// route — and [`ENV_AXIS_VETO_SUBSTRINGS`] keeps the rejection-verb family from crossing into the
/// env axis, which is a different rule's question.
///
/// Known limits, deliberately left as UNDER-recognition (the finding still fires, a human still
/// looks — the opposite error silently hides a real missing gate):
/// - Composition helpers (`compose(requireAdmin, handler)`, `pipe(...)`) are not unwrapped.
/// - A wrapper named by a pure narrowing adverb (`adminOnly(h)`) or by no guard vocabulary at all
///   (`gate(h)`, `wrap(h, 'admin')`) is not recognized; a Mode B overlay injecting `auth-guarded`
///   remains the escape hatch.
/// - The wrapped inner handler is NOT lifted into the entry's `handler` field, so call-graph BFS
///   still cannot see through the wrapper — unchanged by this judgment.
///
/// `e` is expected to already be unwrapped (see `unwrap_expr`) by the caller.
pub(super) fn judge_guard_wrapper_arg(e: &Expr) -> bool {
    let Expr::Call(call) = e else {
        return false;
    };
    if judge_guard_arg(e) {
        return true;
    }
    let Some(dotted) = callee_dotted(call) else {
        return false;
    };
    let tail = name_tail(&dotted);
    // The sub-router/DI veto applies here too: `protectedRoutes(x)` is a router factory, not a gate.
    if ROUTER_NAME_VETO_SUFFIXES.iter().any(|s| tail.ends_with(s))
        || ENV_AXIS_VETO_SUBSTRINGS.iter().any(|s| tail.contains(s))
    {
        return false;
    }
    WRAPPER_GUARD_PREFIXES
        .iter()
        .any(|p| tail.len() > p.len() && tail.starts_with(p))
}

/// A call's callee as dotted text (`passport.authenticate`), or None for a non-expression callee
/// (`super(...)`, `import(...)`) or an unnameable callee shape.
fn callee_dotted(call: &CallExpr) -> Option<String> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    handler_name(unwrap_expr(callee))
}

/// The judged name: the segment after the last `.`, lowercased. The OBJECT half of a dotted chain is
/// deliberately never judged — it names WHERE the function lives, not what it does, so
/// `admin.list`/`auth.handlers` must not read as guards.
fn name_tail(dotted: &str) -> String {
    dotted
        .rsplit('.')
        .next()
        .unwrap_or(dotted)
        .to_ascii_lowercase()
}

/// [`is_guard_name`] applied to `dotted`'s tail, vetoed first by [`ROUTER_NAME_VETO_SUFFIXES`].
fn is_guard_tail(dotted: &str) -> bool {
    let tail = name_tail(dotted);
    if ROUTER_NAME_VETO_SUFFIXES.iter().any(|s| tail.ends_with(s)) {
        return false;
    }
    is_guard_name(&tail)
}
