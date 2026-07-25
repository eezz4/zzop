//! The `mutating-route-no-auth` finding message, split out of the parent module purely for size (the
//! 300-line source cap) — same convention as `unprovided_consume/message.rs`.
//!
//! What the message must carry, and why it is this long: this rule's exemptions are DECIDED BEFORE the
//! BFS runs, so for an exempt route the rule emits nothing at all. Silence therefore has two very
//! different readings ("checked, no gap" vs "never checked"), and only the message can tell them apart —
//! every pre-BFS exemption in [`super::scan_mutating_route_no_auth`] is disclosed here for that reason.
//! Keep this in sync with `docs/rules/catalog.md` and `site/rules.html`'s row for this rule.

use zzop_core::disable_hint;

use super::CALL_GRAPH_COVERED_EXTENSIONS;

/// Builds the finding message (also stored as `data.hint`) for one unguarded mutating route.
pub(super) fn missing_auth_hint(
    method: &str,
    path: &str,
    handler_ref: &str,
    auth_guard_pattern: &str,
) -> String {
    // Quoted verbatim so the published sightline can never drift from `super::is_call_graph_covered`.
    let covered_exts = CALL_GRAPH_COVERED_EXTENSIONS.join("/");
    format!(
        "{method} {path} (handler `{handler_ref}`) never reaches a call whose name looks like an auth \
         guard ({auth_guard_pattern}) anywhere in its call graph — this mutating route may be missing an \
         authorization check. Add an explicit, named guard call reachable from the handler (e.g. \
         requireAuth(), verifySession()), or confirm auth is actually enforced. Exemption: routes whose \
         path is itself on the auth-acquisition surface are never checked by this rule, since that \
         surface cannot require pre-existing auth to reach itself — either a standalone segment \
         (`/auth/...`, `/login`, `/logout`, `/signin`, `/signup`), or a segment like `/register`, \
         `/token`, `/refresh`, `/password`, `/otp` PAIRED WITH an auth-family segment elsewhere in the \
         same path (e.g. `/auth/register` is exempt, but `/devices/register` is NOT — `register` alone \
         isn't enough). A route registered in a test/fixture file (`__tests__/`, `__test__/`, `tests?/`, \
         `spec/`, `*.test.*`, `*.spec.*`, and similar per-language conventions) is also never checked — \
         a route only ever defined/called from a test is not exposed application surface. LANGUAGE \
         SIGHTLINE, the third pre-BFS exemption and the one easiest to misread as a verdict: only a route \
         whose registration file carries a call-graph-covered extension ({covered_exts}) is checked at \
         all, because the symbol graph this BFS walks is built from those alone — a Python, Go, Rust or \
         C# mutating route never enters the BFS, since there \"never reaches a guard\" would be \
         guaranteed by the empty graph rather than evidence about the route. So ZERO findings of this \
         rule in a repo outside those extensions means NOT ANALYZED, never \"no missing auth\". \
         Precision limit: this is a call-graph-BFS, vocabulary-based check — route-level middleware (e.g. \
         `apiRoutes.post(\"{path}\", requireAuth, {handler_ref})`, or a router-wide `.use(authMiddleware)`) \
         never appears as a call FROM the handler itself, so it is invisible to this check and WILL \
         false-positive on a route guarded only that way — this finding starts at Info severity until \
         this check becomes middleware-aware. {} if your auth happens at the middleware layer (this rule \
         has no inline suppression marker).",
        disable_hint("mutating-route-no-auth")
    )
}
