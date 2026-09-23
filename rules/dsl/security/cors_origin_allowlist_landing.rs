//! §33/§37 LANDING for the three rules whose remedy is "stop being permissive about ORIGIN and
//! enumerate it instead" — `security/{cors-wildcard,cors-credentials-wildcard,
//! cors-reflected-origin-credentials}`.
//!
//! WHAT THE READER'S CORRECT EDIT COSTS (`1.architecture/rules/rule-quality.md` §27 leg 3). All three
//! end in the same move: replace `*`, or a reflected `Origin`, with a list of origins the author
//! writes down. That list is a completeness obligation the author did not have a moment earlier, and
//! it is the only remedy in this pack whose failure happens in a browser the author does not run —
//! curl, a server-side suite and every origin they did remember all keep passing. Worse for a simple
//! GET/POST, which takes no preflight: the browser sends the request anyway and withholds only the
//! RESPONSE, so the write commits and the page reports a failure. The second half is the cache: an
//! `Access-Control-Allow-Origin` chosen per request is a varying header, and without `Vary: Origin` a
//! shared cache or CDN hands one origin's value to the next caller — which either breaks every other
//! origin or, with credentials in play, is the leak the edit was meant to close.
//!
//! ONE CONSTANT FOR THE THREE, byte-identical. The property belongs to an enumerated origin list as
//! such, not to which permissive spelling a rule detects: a literal `*` header, an `origin: '*'`
//! config key and a reflected `req.headers.origin` all land on the same list and the same cache
//! header. The rules diverge only in their EXIT — which is each rule's own imperative and stays in
//! each rule's own message. Same split `ROTATION_LANDING` uses for its eight and
//! `SANITIZER_SUBTRACTION_LANDING` for its seven.
//!
//! WHY `URL_SCHEME_ALLOWLIST_LANDING` IS NOT REUSED — §37's most-dangerous-reuse test, and it is the
//! closest sibling in the tree because both nouns are the words "allow-list" and "URL". Its list is
//! over SCHEMES on an href in the reader's OWN document, what it over-rejects is a relative path, a
//! `mailto:` or a `blob:` target the same page authored, and its failure is a link that stops being a
//! link. This list is over OTHER PEOPLE'S origins, what it drops is a caller the author never
//! enumerated, and its failure happens in that caller's browser. Splicing that constant here would
//! hand a reader configuring a server a paragraph about anchor elements.
//!
//! WHY `TRUST_STORE_LANDING` IS NOT REUSED EITHER. It shares one leg — "it fails at runtime rather
//! than at build" — and nothing else. Its subject is this process's OWN outbound TLS, where the
//! failure is an exception in this stack trace. Here the failing party is a third party's page, which
//! is why the sentence has to say where to look rather than what to catch.
//!
//! NOT A DISQUALIFIER (§38's two axes). None of the three findings becomes wrong because the list is
//! hard to keep complete: `*` with credentials is still rejected by browsers, a reflected origin is
//! still trust-anything, and a public wildcard is still a wildcard. `cors-wildcard` states its own
//! disqualifier separately (a purely public, credential-less API) and keeps its axis-A pin unchanged;
//! `cors-credentials-wildcard` keeps its `ORDER_CLAIMS` row; `cors-reflected-origin-credentials`
//! keeps its `NO_DISQUALIFIER` row, whose category gains "+ landing" for the reason the three restored
//! rows in `message_order_verdicts.rs` record — the row said there was no limitation prose and the
//! landing adds prose that still disqualifies nothing.
//!
//! POSITION, not presence. The invalidation probe for every carrier is to move this constant to the
//! tail of that rule's message: every token stays present and spelled exactly once, and each pin must
//! go red on ORDER alone.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

const CORS_ORIGIN_ALLOWLIST_LANDING: &str = "AN ENUMERATED ORIGIN LIST IS ONE YOU HAVE TO KEEP COMPLETE, AND WHAT IT DROPS FAILS IN SOMEBODY ELSE'S BROWSER RATHER THAN IN YOUR TESTS: curl and a server-side suite pass either way, while a preview deployment, a partner's embed or a native WebView you did not think of gets the response withheld — and on a simple GET/POST there is no preflight, so the browser had already sent the request and the write already happened, leaving the page to report a failure the database recorded as a success. List the origins that call this endpoint today rather than the ones you can name, and where the header is chosen per request send `Vary: Origin` with it, or a shared cache or CDN hands one origin's `Access-Control-Allow-Origin` to the next caller.";

/// `cors-wildcard`. Its axis-A pin (`the_public_api_exemption_precedes_the_imperative`) asserts the
/// public-API exemption precedes the same imperative; this pin is the other axis, in a `fn` of its own
/// so neither block's helper set decides the other's verdict.
#[test]
fn cors_wildcard_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { origin: '*' };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-wildcard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "cors-wildcard",
        &h[0].message,
        CORS_ORIGIN_ALLOWLIST_LANDING,
        "Return a specific allow-listed origin instead,",
    );
}

/// `cors-credentials-wildcard`. Its axis-A verdict is an `ORDER_CLAIMS` row against the message
/// TEMPLATE; this is a DELIVERED finding, and the two make different claims about different orders.
#[test]
fn cors_credentials_wildcard_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = {\n  origin: '*',\n  credentials: true,\n};\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-credentials-wildcard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "cors-credentials-wildcard",
        &h[0].message,
        CORS_ORIGIN_ALLOWLIST_LANDING,
        "Return a specific allow-listed origin (never `*`",
    );
}

/// `cors-reflected-origin-credentials`. This is the arm the landing is sharpest on: the reflection it
/// reports is what a team reaches for BECAUSE an enumerated list was incomplete, so a reader sent back
/// to that list without being told what it costs is being sent back to the reason they left it.
#[test]
fn cors_reflected_origin_credentials_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cors.ts",
        "export const corsOptions = { origin: true, credentials: true };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "cors-reflected-origin-credentials");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "cors-reflected-origin-credentials",
        &h[0].message,
        CORS_ORIGIN_ALLOWLIST_LANDING,
        "Return a specific allow-listed origin instead of reflecting",
    );
}

/// BYTE-IDENTITY, asserted rather than assumed — the whole value of a constant over three authored
/// copies. A paraphrase in any one of them would still read fine to a human and would silently
/// un-index that rule's position pin.
#[test]
fn all_three_carriers_splice_the_same_bytes() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("api/a.ts", "export const corsOptions = { origin: '*' };\n");
    dir.write(
        "api/b.ts",
        "export const withCreds = {\n  origin: '*',\n  credentials: true,\n};\n",
    );
    dir.write(
        "api/c.ts",
        "export const reflected = { origin: true, credentials: true };\n",
    );
    let out = scan(&dir);
    for rule in [
        "cors-wildcard",
        "cors-credentials-wildcard",
        "cors-reflected-origin-credentials",
    ] {
        let h = hits(&out, rule);
        assert!(!h.is_empty(), "{rule} did not fire: {:?}", out.findings);
        assert_eq!(
            h[0].message.matches(CORS_ORIGIN_ALLOWLIST_LANDING).count(),
            1,
            "{rule}: the shared landing must appear exactly once, byte-identical. In: {}",
            h[0].message
        );
    }
}
