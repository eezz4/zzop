//! The `mutating-route-no-auth` finding message, split out of the parent module purely for size (the
//! 300-line source cap) — same convention as `unprovided_consume/message.rs`.
//!
//! What the message must carry, and why it is this long: this rule's exemptions are DECIDED BEFORE the
//! BFS runs, so for an exempt route the rule emits nothing at all. Silence therefore has two very
//! different readings ("checked, no gap" vs "never checked"), and only the message can tell them apart —
//! every pre-BFS exemption in [`super::scan_mutating_route_no_auth`] is disclosed here for that reason.
//! Keep this in sync with `docs/rules/catalog.md` and `site/rules.html`'s row for this rule.
//!
//! ## Why the counter-indications come BEFORE the imperative
//! This rule's remedy is a prescription, and several route shapes make following it a regression: a
//! caller that legitimately holds no session gets 401'd by the very guard this rule asks for. Ordering
//! is load-bearing, not stylistic: an external auditor's complaint about the sibling rule that closed
//! this same class was that the imperative was the first code token and the caveat sat thousands of
//! characters later, where a reader acting on the first instruction never reaches it. The pins
//! `the_remedy_warns_off_third_party_callback_receivers_before_it_asks_for_a_guard` and
//! `the_remedy_warns_off_account_less_callers_and_wrapped_handlers_before_it_asks_for_a_guard` assert
//! POSITION, not just presence. Every clause names its shape by ROLE only — no vendor, path or corpus
//! tree, since the findings that motivated them are single routes in measured repos and a message that
//! quoted one would be right for the wrong reason.
//!
//! ## Why the enumeration says it is an enumeration
//! The first version of this block listed ONE exempt shape (the callback receiver) and closed with
//! "Everywhere else, the remedy is the direct one". An uncontaminated auditor read a real finding
//! through it — an account-less person cancelling from a link in a confirmation email — and named the
//! defect precisely: a narrow list that closes with "everywhere else, just do it" turns its own
//! INCOMPLETENESS INTO A PUSH, promoting every unlisted legitimate shape from "not covered" to
//! "confirmed". That is worse than shipping no exception at all, because the reader now believes the
//! question was asked and answered. So the closing sentence has to say the list is examples, and hand
//! the reader the discriminator (who is the intended caller, can that caller hold a session) plus the
//! two signals that answer it from the code — an opaque high-entropy identifier as the subject, and
//! compensating controls already present.
//!
//! ## Why the WRAPPED HANDLER clause is here and not in the precision-limit tail
//! The BFS starts at the EXPORTED route symbol. When that symbol is a wrapper call taking the handler
//! as an ARGUMENT (`export const POST = withX(handler)`), the argument draws no call edge
//! (`RawCall` is produced from callee positions only), so the walk never enters the handler body and
//! this rule's central claim was never tested. Measured: 15 of 151 corpus firings sit on that shape and
//! at least 5 of them call a guard matching this run's own vocabulary one hop inside. That makes it a
//! reason the finding is FALSE, not a footnote about reach — so it goes before the imperative with the
//! route shapes, and `data.unresolvedCallees` cannot carry it (the argument was never a dropped CALL,
//! so that channel has nothing to report).

use zzop_core::disable_hint;

use super::CALL_GRAPH_COVERED_EXTENSIONS;

/// Builds the finding message (also stored as `data.hint`) for one unguarded mutating route.
pub(super) fn missing_auth_hint(
    method: &str,
    path: &str,
    handler_ref: &str,
    auth_guard_pattern: Option<&str>,
    unresolved: &[&str],
) -> String {
    // Present only when this handler HAS such calls — the additive-disclosure convention the `data` key
    // beside it follows. The names are already known not to match the guard pattern (they are the
    // residue after the clearing check), so this clause says what the walk could not see, never what it
    // decided. Naming them is what turns "work out why the graph is short an edge" into one glance.
    let unresolved_clause = if unresolved.is_empty() {
        String::new()
    } else {
        format!(
            " Calls this handler makes that the resolver could NOT place, and whose names do not match \
             the guard vocabulary either: {}. They drew no call-graph edge, so nothing downstream of \
             them was walked — if your guard is spelled among these, this finding is a false positive \
             and the fix is the vocabulary key, not the route.",
            unresolved.join(", ")
        )
    };
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
         authorization check. THREE THINGS TO RULE OUT FIRST — for each of them the direct remedy is \
         wrong. (1) WRAPPED HANDLER: the walk starts at the EXPORTED route symbol, so a handler \
         passed as an ARGUMENT to a wrapper (`export const POST = withX(handler)`) draws no call \
         edge — the walk never enters the handler body, and this rule's claim was never tested \
         there. Open the handler: if it already calls your guard, this finding is false. \
         (2) INBOUND CALLBACK RECEIVER: a payment-notification \
         or webhook endpoint a third party's SERVER posts to. That sender has no session and none \
         of your credentials, so an auth guard here answers it 401: the delivery is dropped after \
         its retries and whatever it settles (a paid order) never lands. What belongs on such a \
         route is signature/HMAC verification of the callback body against the provider's shared \
         secret, plus replay handling. \
         (3) A CALLER WITH NO ACCOUNT holding a high-entropy link: a cancel-or-confirm link in a \
         notification email, a magic-link or password-reset landing, a guest checkout, an \
         invitation. The unguessable id in the request IS the credential, so a session guard locks \
         out every legitimate recipient — they never had an account to log in to. What belongs \
         there: accept the opaque high-entropy id and never a sequential one, check a CSRF token, \
         rate-limit by IP, and scope each write to the object that id names. \
         (2) AND (3) ARE EXAMPLES, NOT THE WHOLE LIST: this rule cannot tell an \
         unauthenticated-BY-DESIGN route from an unguarded one, so name the intended caller first \
         and say whether that caller can hold a session at all — if it cannot, the correct edit is \
         none. The code answers that faster than reasoning does: an opaque high-entropy identifier \
         as the handler's subject, and compensating controls already in place. Where the intended \
         caller DOES hold a session, the remedy is the direct one. \
         Add an explicit, named guard call reachable from the handler (e.g. \
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
         the JS/TS extensions a call is resolved across files, so the phrase is literal, and since \
         2026-09-07 it is literal for `.java` too — a Java import specifier is a dotted package/class \
         name that resolves to ITSELF, which used to stop the walk one hop out, and the whole-corpus \
         type index is now bridged onto the graph so the walk continues. It is NOT literal for a \
         Python module-attribute receiver (`from pkg import mod; mod.f()`): that receiver is read as \
         a class, so the walk stops one hop out and a guard reached as handler -> helper in another \
         file -> guard is NOT found. In that one case a finding means \"no guard within one hop\", never \
         \"no guard anywhere\".{unresolved_clause} {} if your auth happens at the middleware layer or \
         beyond that hop bound (this rule has no inline suppression marker).",
        disable_hint("mutating-route-no-auth")
    )
}
