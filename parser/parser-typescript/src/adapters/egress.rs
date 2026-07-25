//! FE HTTP egress IO extractor — projects the routes a TS/JS tree CONSUMES, so the core cross-layer
//! linker can join each call to its backend handler. Absolute-URL calls are also projected, with a
//! host-carrying key, so they surface as third-party egress instead of being dropped.
//!
//! The crux is constant indirection: a frontend rarely writes `axios.get("/x")`; it writes
//! `axios.get(ControlKey.AUTHEN.getUserInfo)`. We build a project-wide constant map from every
//! top-level object literal first, then resolve each call's URL against it, normalized via
//! `core::http_consume_interface_key` (which also drops any `?...`/`#...` query/fragment suffix —
//! `query-drop-v1`) so the key matches whatever the BE adapter emits.
//!
//! Recognized call shapes: `axios.get/post/put/delete/patch(url)`, `ky.get/post/...(url)`,
//! `fetch(url, { method })`, `$fetch(url, { method })`, `axios(url)`, and a computed member callee on
//! `axios`/`ky` (`axios['post'](url)`, `axios[favorited ? 'delete' : 'post'](url)`) whose bracket
//! expression is a recognized-verb string literal or a ternary with two such literal arms, and
//! Angular's `this.<name>.get/post/put/delete/patch(url)` / `<name>.get/...(url)` where `<name>` must be
//! HttpClient-typed (constructor param property or class property) or `inject(HttpClient)`-initialized
//! in a file that itself imports `@angular/common/http` (see `angular-httpclient-v1` below).
//! Generated-SDK clients (e.g. oazapfts) are NOT recognized here — that vocabulary lives in an
//! injection adapter (`examples/oazapfts-adapter`), not the engine (decision: generated SDKs are
//! injection adapters, not engine vocab).
//!
//! `cond-literal-fanout-v1`: a ternary with two string-literal arms — as the whole URL argument, as a
//! template interpolation, or as the computed-member method — enumerates one deterministic key per arm
//! instead of collapsing to `{}`/going unrecognized. Both arms are visible literals in the source, so
//! this is normalization of visible facts, not speculation (the "never guess" convention only forbids
//! inventing values that aren't written down). Template fan-out is capped at 2 conditional-literal
//! interpolations (≤4 variants); a 3rd+ interpolation of that shape falls back to `{}` for ALL of them
//! in that template, keeping output bounded and deterministic.
//!
//! `angular-httpclient-v1`: Angular's dependency-injected `HttpClient` idiom — `this.<name>.get/post/
//! put/delete/patch(url)` or `<name>.get/...(url)` — is recognized only when `<name>` is a proven
//! HttpClient receiver in THIS file: a constructor parameter property typed `HttpClient`, a class
//! property typed `HttpClient`, or a class property/local `const`/`let` initialized with
//! `inject(HttpClient)`, gated on the file itself importing (any specifier) from
//! `@angular/common/http` — never guessed from the bare name `http` alone. Over-approximation WITHIN a
//! gated file is accepted: resolution is per-file, not per-class, so two same-named-but-differently-typed
//! receivers in one gated file both match. `request(method, url)` is out of scope for v1.
//!
//! `str-concat-url-v1`: binary `+` string concatenation (`'/profiles/' + username`, `'/profiles/' +
//! username + '/follow'`) is the isomorphic counterpart to template-literal resolution. The
//! left-associative `+` chain is flattened into its operands; the whole chain is rejected (unresolved)
//! if any operator in it is not `+` (a `-`/`??`/`||` chain is never guessed) or if NO operand is a direct
//! string literal (a fully-dynamic `base + path` stays unresolved, same as today). Each operand maps to
//! the same `TplPiece` vocabulary as template resolution: a string literal or resolved const is `Fixed`;
//! a ternary with two string-literal arms is a `Slot` (cartesian fan-out, capped at 2 slots — same
//! bounded-output rule as `cond-literal-fanout-v1`); anything else falls back to the old `{}`
//! placeholder.
//!
//! `same-file-const-prepend-v1`: a URL's LEADING slot — `` `${BASE}/x` `` or `BASE + '/x'` — resolves
//! against a bare `const`/`let` constant declared in the SAME file, when that name clears four never-guess
//! gates (single binding in the file, no reassignment, no parameter/destructuring shadow, plain string
//! literal initializer); see [`binding_census`] for the first three and [`local_consts`] for the fourth,
//! plus why the project-wide `consts` map still refuses bare names. Head only: a mid-path interpolation is
//! a route parameter and `{}` is its correct normalization, whereas at the head `{}` forces
//! `consume_key_for`'s base-carrier head-drop to ASSUME the base carries no scheme or host — which
//! silently files a third-party call as an internal route when the constant is an absolute URL. Any gate
//! failure leaves the old opaque-`{}` behavior untouched.
//!
//! `same-file-url-binding-v1`: the same file, the OTHER position — a bare identifier standing alone as the
//! whole url argument (`fetch(URL)`, `fetch(url, { method })`) resolves to whatever its same-file binding's
//! initializer resolves to, ONE hop, and only when that initializer resolves on its own
//! (`const url = ``${BASE}/x`` ` does; `const url = other` and `const base = process.env.X` do not). The
//! same [`binding_census`] gates apply, so the name is proven to be bound exactly once in this file.
//! Unlike the head slot this position has nothing to assume — there is no head-drop bucket in front of it —
//! so the substitution is exactly equivalent to finding that expression written at the call site. Nesting
//! is not a gate here: a function-local `const url = …` qualifies like a top-level one, because "bound
//! exactly once in the file" is what carries the scope argument. Interprocedural resolution (a wrapper
//! parameter, a component prop) remains out of scope — that is value resolution, not constant reading.

mod angular;
mod binding_census;
mod body_shape;
mod collector;
mod concat;
mod consts;
mod correlation;
mod generated_client;
mod keying;
mod local_consts;
mod matchers;
mod object_shape;
mod react_query;
mod retry;
mod url_resolve;
#[cfg(test)]
mod url_resolve_tests;

pub use collector::extract_http_egress;
pub use consts::{const_map_fragment, resolve_raw_path};
pub use keying::{base_relative_path, is_external_url};

use swc_core::ecma::ast::Expr;

/// Strip wrappers between a declaration and its real value: `... as const`, `(...)`, `... satisfies T`, `...!`.
fn unwrap_expr(e: &Expr) -> &Expr {
    let mut n = e;
    loop {
        n = match n {
            Expr::TsAs(a) => &a.expr,
            Expr::TsConstAssertion(c) => &c.expr, // `... as const`
            Expr::Paren(p) => &p.expr,
            Expr::TsSatisfies(s) => &s.expr,
            Expr::TsNonNull(nn) => &nn.expr,
            other => return other,
        };
    }
}

/// Shared test scaffolding for the submodules' `tests` mods (visible to descendants only).
#[cfg(test)]
fn files(xs: &[(&str, &str)]) -> Vec<(String, String)> {
    xs.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[cfg(test)]
fn keys(out: &[zzop_core::IoConsume]) -> Vec<Option<String>> {
    out.iter().map(|c| c.key.clone()).collect()
}

#[cfg(test)]
fn clients(out: &[zzop_core::IoConsume]) -> Vec<Option<String>> {
    out.iter().map(|c| c.client.clone()).collect()
}
