//! Manual pathname-dispatch route provides: framework-less servers (raw Cloudflare Workers,
//! Node `http.createServer`, Deno/Bun serve) that route by comparing `url.pathname` against
//! string literals (`if (url.pathname === "/x")` chains, `switch (url.pathname)`), instead of a
//! decorator/router-registration vocabulary. swc-AST-based (mirrors `controller_decorators.rs`'s
//! per-file visitor shape), never-guess on every axis called out below.
//!
//! ## Per-function evidence gates (both required before any emission from that function)
//! 1. **Request context**: a param with TS type annotation `Request`, or named exactly `request`
//!    or `req` (covers untyped JS). Checked once per function signature; a function with neither
//!    contributes nothing at all (cheaper than gating each path test individually, and matches
//!    the false-positive corpus: `location.pathname`/`new URL(window.location.href)` sites live
//!    in client code that never takes a `request`/`req` param). Known residual FP of the
//!    name-based half: a service-worker fetch HELPER (`function onFetch(request) { const url =
//!    new URL(request.url); ... }`) passes both gates and emits its offline/cache routes as
//!    provides even though a service worker is not a server — accepted v1 tradeoff; revisit if
//!    a PWA corpus pulls (a `self.addEventListener` file-level veto is the likely fix).
//! 2. **URL provenance** for the pathname receiver actually compared: `<u>.pathname` where `<u>`
//!    is a `URL`-typed param or a local `const/let/var <u> = new URL(...)`, or a local alias
//!    (`const { pathname } = <u>`, incl. rename, or `const p = <u>.pathname`). A receiver that is
//!    itself a member-of-member (`request.nextUrl.pathname`) is deliberately NOT provenanced —
//!    only a bare-identifier receiver qualifies, which is what excludes Next middleware's
//!    `request.nextUrl.pathname` and any `router.pathname`/`location.pathname` shape.
//!
//! The real-world anchor: a dispatch function commonly receives `url: URL` as a typed parameter
//! injected by a cross-file wrapper rather than constructing it locally — gate 2 accepts a
//! `URL`-typed PARAM for exactly this reason, not just a same-function `new URL(...)`.
//!
//! ## Durable Object veto
//! An entire class body is skipped (no methods analyzed, DO or not) when the class has DO
//! evidence: a constructor param typed `DurableObjectState`, or an `implements`/`extends` clause
//! naming `DurableObject`. A Durable Object's `fetch()` routes are reachable only via
//! `stub.fetch` — an edge request 404s — so emitting them as `kind:"http"` provides would
//! over-claim public surface. The veto is types/`extends`-gated only: an untyped plain-JS DO
//! (`constructor(state, env)`, no clause) is undetectable without types and DOES emit its
//! internal routes — accepted v1 limit, documented rather than guessed around.
//!
//! ## Verb recognition
//! A verb mention is a binary comparison (`===`/`==`/`!==`/`!=`, either operand order) between a
//! string literal exactly matching `zzop_core::HTTP_KEY_VERBS` and either `<r>.method` (`<r>`
//! request-evidenced) or a local method alias (`const method = <r>.method` / `const { method } =
//! <r>`); or a `switch (<r>.method | alias)` case with such a literal. `!==` counts — same
//! mentioned-verb semantics as `next_pages_api.rs`'s `req.method !== "POST"` early-return.
//!
//! ## Path test
//! `===`/`==` (either operand order) between a pathname-provenanced receiver and a string literal
//! starting with `/` (a zero-interpolation template literal counts, cooked). Deliberately
//! excluded (v1 under-approximation): `!==`/`!=` path guards, `startsWith`/`includes`,
//! interpolated templates, literals without a leading `/`, const indirection through an
//! unresolved identifier.
//!
//! ## Regex path test (`pathname.match(/re/)`)
//! Raw-Worker parameterized routes commonly dispatch by regex, either bound
//! (`const m = pathname.match(/^\/api\/x\/([^/]+)$/); if (m && method === "POST") ...`) or inline
//! (`if (pathname.match(/re/) && ...)`), optionally guarded `!== null`/`!= null`. The regex SOURCE
//! is converted to a route path by `regex_path::regex_to_route_path` and keyed exactly like a
//! literal path — `http_interface_key`'s `{x}`/`:x` -> `{}` normalization makes a converted `{}`
//! param join the FE template-literal consume side. The converter is strictly never-guess: it
//! requires full `^...$` anchoring, no flags, and every `\/`-split segment to be either a clean
//! literal or one of a fixed allowlist of single-path-segment param matchers (`([^/]+)`, `(\d+)`,
//! `(\w+)`, common char classes, capturing or non-capturing). Alternation, optional segments, a
//! `.+` multi-segment catch-all, named groups, or any unrecognized shape make the WHOLE regex bail
//! (no provide — the site stays in the honesty channel, never a wrong key). `new RegExp("...")` (a
//! string, not a `/re/` literal) is not handled.
//!
//! ## Association algorithm
//! Per function body (independently — bindings/tests never leak across a nested function
//! boundary): every `IfStmt` reachable without crossing into a nested function is evaluated on
//! its own test, decomposed into `&&`-conjuncts (recursively unwrapping parens). Each conjunct is
//! either a path test, a verb test, or an `||` disjunction of same-shaped tests (an all-path `||`
//! contributes every disjunct's path; an all-verb `||` unions its verbs; a MIXED `||` — e.g.
//! `(path || flag)` — contributes nothing, never guessed). If the resulting path set is
//! non-empty: verbs come from the test's own conjuncts if any were found there, else from
//! recursively scanning the `if`'s consequent block for verb mentions (if-conditions,
//! switch-on-method) — stopping at nested function bodies and skipping the whole subtree of any
//! nested `IfStmt` whose OWN condition contains a path test (that nested `if` is a separate
//! route, evaluated independently; letting its verb scan leak into the parent would
//! cross-contaminate two different routes) — else a single `zzop_core::UNKNOWN_VERB` sentinel (the
//! path is served, method unknown — the engine lifts it to the `cross-layer/unknown-verb-route`
//! disclosure). One provide is emitted per (path × verb); `line` is the path test's own line.
//!
//! ## `symbol` — per-BRANCH, two cases
//! 1. **Branch target**, when the branch's body is a SINGLE statement making EXACTLY ONE call, to a
//!    bare-identifier callee (`if (m && method === "POST") return createGroup(request, env);`, plus
//!    its one-statement-block, `await` and paren wrappings, and a bare expression statement): that
//!    CALLEE (`createGroup`) is the route's real handler and becomes the symbol.
//! 2. **Enclosing function**, for anything else — a multi-statement block, an inline arrow, an
//!    unnameable expression (`new Response(...)`), a member callee (`handlers.create(...)`, where
//!    picking one property off the chain would be a guess), or a WRAPPER call whose arguments
//!    themselves invoke something (`return ok(handleThing(req));`,
//!    `return json(await createGroup(req, env));`). Its name when nameable (`FnDecl` ident,
//!    class-method/object-method key, or `const name = () => {}` binding name).
//!
//! Case 1 is the SYMBOL-axis twin of the verb-axis cross-contamination guard above. A dispatch
//! function serves N routes from one body, so pinning all N provides to `dispatch` makes any
//! consumer that walks the call graph FROM `symbol` — THREE native rules start a BFS there:
//! `mutating-route-no-auth`, `unsafe-read-endpoint`, and `non-idempotent-write` — see the UNION of
//! every sibling route's reachability. Measured on a real Worker repo: one sibling calling a
//! guard-named function silenced `mutating-route-no-auth` for genuinely unauthenticated siblings,
//! and a GET route "reached" an INSERT that lives in a POST sibling's handler in another file.
//!
//! ### Case-2 residual: an ARGUMENT-COERCION call also blocks attribution
//! "arguments themselves invoke something" is a purely lexical test, so it fires on
//! `return getRevision(request, env, m[1], Number.parseInt(m[2], 10));` too — there the outer callee
//! IS the handler and the inner call only coerces an argument, yet the branch still falls back to the
//! enclosing dispatcher. This is deliberate, not an oversight: nothing lexical separates "the outer
//! call is a response wrapper hiding the real handler" from "the outer call is the handler and the
//! inner one is a coercion", and getting that backwards on the wrapper shape makes
//! `mutating-route-no-auth` ACCUSE a guarded route (case 2's whole reason to exist). Falling back
//! only ever over-reaches. Measured cost, 2026-07-25 mono-hub: of 6 `unsafe-read-endpoint` findings
//! that per-branch attribution was expected to clear, exactly 1 survived on this shape
//! (`settle-hub-be` `GET /api/ledger/{}/revision/{}`, whose handler call parses a numeric path
//! segment inline). Pinned by `tests/branch_symbol.rs`.
//!
//! Case 2 never guesses: an unattributable branch keeps the old, honest-if-coarse enclosing symbol.
//! The wrapper-call exclusion is why "exactly one call" is load-bearing rather than pedantic —
//! taking the OUTERMOST callee of `return ok(createGroup(req, env));` would aim all three BFSes at
//! `ok`, which reaches neither the guard nor the write inside `createGroup`. That is not coarse,
//! it is wrong in a new direction: `mutating-route-no-auth` would newly ACCUSE a guarded write
//! route. The enclosing symbol only ever over-reaches, exactly as it did before this rule existed.
//!
//! A FOURTH consumer reads `symbol` by EQUALITY rather than by BFS: `duplicate-route` skips a
//! repeat registration that resolves to the SAME handler symbol as the first (the
//! trailing-slash-tolerance idiom is not a shadow). Case 1 therefore makes that rule newly fire
//! when two branches of one dispatcher register the same key with two different handlers — correct
//! (the second branch is genuinely dead), and a deliberate detection increase pinned e2e in
//! `crates/engine/tests/analyze_routes_pathname_dispatch.rs` together with the case-2 control that
//! must stay silent.
//!
//! A `SwitchStmt` whose discriminant is a pathname-provenanced receiver is handled the same way,
//! grouping consecutive empty-body cases onto the next non-empty body (fallthrough), scanning
//! that shared body for verb mentions (else fallback), with `line` = the case's own line. The
//! grouped body is also the branch body for the `symbol` rule above — every case path in a group
//! is genuinely served by it, so sharing its derived symbol is not cross-contamination.
//!
//! Exact-duplicate `(key, line, symbol)` triples are deduped; output order is deterministic
//! (occurrence order).
//!
//! ## Pre-gate deviation
//! The pre-gate checks for the bare substring `"pathname"`, not a literal `".pathname"`. The
//! canonical `const { pathname } = url; if (pathname === ...)` shape (module doc gate 2) never
//! spells a dot before `pathname` anywhere in the file — a literal `".pathname"` substring gate
//! would reject that required shape outright. `"pathname"` alone is still a cheap, useful
//! fast-path (a file that never mentions the word at all cannot match any recognized shape).

use std::collections::HashSet;

use swc_core::common::SourceMap;
use swc_core::ecma::visit::VisitWith;
use zzop_core::IoProvide;

use collector::TopCollector;

mod classify;
mod collector;
mod ctx;
mod regex_path;
mod routes;
#[cfg(test)]
mod tests;

/// Extract `kind:"http"` route provides from manual pathname-dispatch sites in one file. See
/// module doc for the full recognizer spec. Returns an empty `Vec` (never panics) on an
/// unparseable file, same convention as every other swc-AST adapter in this crate.
pub fn extract_pathname_dispatch_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    // Cheap pre-gate: every recognized shape mentions "pathname" somewhere — either a direct
    // `.pathname` member access, or (deliberately widened from a literal ".pathname" substring
    // check — see module doc "Pre-gate deviation") a destructured/aliased bare `pathname`
    // identifier, which the canonical `const { pathname } = url` shape never spells with a dot.
    if !text.contains("pathname") {
        return Vec::new();
    }
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut collector = TopCollector {
        cm: cm_ref,
        rel,
        out: Vec::new(),
        pending_name: None,
    };
    module.visit_with(&mut collector);
    dedup_provides(collector.out)
}

fn dedup_provides(provides: Vec<IoProvide>) -> Vec<IoProvide> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(provides.len());
    for p in provides {
        let sig = (p.key.clone(), p.line, p.symbol.clone());
        if seen.insert(sig) {
            out.push(p);
        }
    }
    out
}

/// A pathname-guarded route whose block names no request-method comparison at all: the path is served
/// but the method is statically unknown. Emit ONE [`zzop_core::UNKNOWN_VERB`] sentinel provide (not a
/// fabricated `[GET, POST]`) — the engine lifts a `"? <path>"` key out of the exact-key join into the
/// verb-unknown disclosure channel (`cross-layer/unknown-verb-route`).
fn fallback_verbs() -> Vec<String> {
    vec![zzop_core::UNKNOWN_VERB.to_string()]
}

fn push_unique(list: &mut Vec<String>, v: String) {
    if !list.iter().any(|x| x == &v) {
        list.push(v);
    }
}
