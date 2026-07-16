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
//! excluded (v1 under-approximation): `!==`/`!=` path guards, `startsWith`/`includes`/regex,
//! interpolated templates, literals without a leading `/`, const indirection through an
//! unresolved identifier.
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
//! cross-contaminate two different routes) — else `PATHNAME_DISPATCH_FALLBACK_VERBS`. One provide
//! is emitted per (path × verb); `line` is the path test's own line, `symbol` is the enclosing
//! function's name when nameable (`FnDecl` ident, class-method/object-method key, or `const name
//! = () => {}` binding name).
//!
//! A `SwitchStmt` whose discriminant is a pathname-provenanced receiver is handled the same way,
//! grouping consecutive empty-body cases onto the next non-empty body (fallthrough), scanning
//! that shared body for verb mentions (else fallback), with `line` = the case's own line.
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
mod routes;
#[cfg(test)]
mod tests;

/// Verbs emitted for a pathname-guarded route whose block names no request-method comparison at
/// all. Same value (and same rationale) as the engine's `PAGES_API_FALLBACK_VERBS` for a
/// `pages/api` handler that names no method literal; the equality is sealed by a cross-crate pin
/// test (policy tier T2), not repeated here.
pub const PATHNAME_DISPATCH_FALLBACK_VERBS: [&str; 2] = ["GET", "POST"];

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

fn fallback_verbs() -> Vec<String> {
    PATHNAME_DISPATCH_FALLBACK_VERBS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn push_unique(list: &mut Vec<String>, v: String) {
    if !list.iter().any(|x| x == &v) {
        list.push(v);
    }
}
