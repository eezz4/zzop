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
    auth_guard_pattern: Option<&str>,
) -> String {
    // Quoted verbatim so the published sightline can never drift from `super::is_call_graph_covered`.
    let covered_exts = CALL_GRAPH_COVERED_EXTENSIONS.join("/");
    // The two readings of this finding are genuinely different and only this clause separates them: a
    // declared pattern means "we looked for these names and found none", while no declaration means the
    // name half of the evidence was never gathered at all. Saying the first when the second is true is
    // the silent-failure class this rule is otherwise built to avoid.
    let guard_clause = match auth_guard_pattern {
        Some(p) => format!("never reaches a call whose name looks like an auth guard ({p})"),
        // The remedy names a document BOTH audiences can obtain, not a subcommand only one of them has:
        // this sentence reaches an MCP client with no argv exactly as often as it reaches a terminal, and
        // the starter config is the same bytes on either side (`packages/cli-bin/src/cli/run.rs`'s
        // `run_init` — one canon behind `zzop init`, `zzop contract config-template`, and MCP
        // `resources/read`), so the pair costs nothing and a CLI-only spelling would strand half the readers.
        None => "declares no `vocabulary.authGuardPattern`, so no call NAME can prove a guard here \
                 (start from the `config-template` contract document — MCP resource \
                 `zzop://contract/config-template` on MCP hosts, `zzop init` with the CLI binary — and \
                 set that key to state how this project spells its guards); the route also reaches no \
                 decorator/annotation guard"
            .to_string(),
    };
    format!(
        "{method} {path} (handler `{handler_ref}`) {guard_clause} anywhere in its call graph — this \
         mutating route may be missing an \
         authorization check. Add an explicit, named guard call reachable from the handler (e.g. \
         requireAuth(), verifySession()), or confirm auth is actually enforced. Exemption: routes whose \
         path is itself on the auth-acquisition surface are never checked by this rule, since that \
         surface cannot require pre-existing auth to reach itself — that surface is whatever this run \
         declared under `vocabulary.authAcquisitionStandalonePattern` (exempt on its own) and \
         `vocabulary.authAcquisitionConditionalPattern` (exempt only alongside \
         `vocabulary.authFamilyPathPattern`, so `/auth/register` is exempt where `/devices/register` is \
         not); an undeclared tier exempts nothing. A route registered in a test/fixture file \
         (`__tests__/`, `__test__/`, `tests?/`, \
         `spec/`, `*.test.*`, `*.spec.*`, and similar per-language conventions) is also never checked — \
         a route only ever defined/called from a test is not exposed application surface. LANGUAGE \
         SIGHTLINE, the third pre-BFS exemption and the one easiest to misread as a verdict: only a route \
         whose registration file carries a call-graph-covered extension ({covered_exts}) is checked at \
         all, because the symbol graph this BFS walks is built from those alone — a mutating route in \
         any OTHER language (Go or C#) never enters the BFS, since there \"never reaches a guard\" \
         would be guaranteed by the empty graph rather than evidence about the route. So ZERO findings \
         of this rule in a repo outside those extensions means NOT ANALYZED, never \"no missing auth\" — \
         and a run that saw such routes says so out loud in its own `warnings` (the call-graph coverage \
         gap self-report names the language and this rule id). \
         Precision limit: this is a call-graph-BFS, vocabulary-based check — route-level middleware (e.g. \
         `apiRoutes.post(\"{path}\", requireAuth, {handler_ref})`, or a router-wide `.use(authMiddleware)`) \
         never appears as a call FROM the handler itself, so it is invisible to this check and WILL \
         false-positive on a route guarded only that way — this finding starts at Info severity until \
         this check becomes middleware-aware. Second precision limit, and the one \"anywhere in its \
         call graph\" above would otherwise overstate: how FAR the walk reaches is per-language. For \
         the JS/TS extensions a call is resolved across files, so the phrase is literal. For `.java` \
         it is not — a Java import specifier is a dotted package/class name and no whole-corpus type \
         index is threaded into this graph, so a specifier resolves to ITSELF and the walk stops one \
         hop out; a guard reached as handler -> helper in another file -> guard is NOT found. The \
         same one-hop bound applies to a Python module-attribute receiver (`from pkg import mod; \
         mod.f()`). In those two cases a finding means \"no guard within one hop\", never \"no guard \
         anywhere\". {} if your auth happens at the middleware layer or beyond that hop bound (this \
         rule has no inline suppression marker).",
        disable_hint("mutating-route-no-auth")
    )
}
