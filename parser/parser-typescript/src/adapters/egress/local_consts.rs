//! Same-file URL bindings — the two rules that let a URL written down in THIS file's own source be READ
//! instead of dropped: `same-file-const-prepend-v1` (the LEADING slot of `` `${BASE}/x` `` / `BASE + '/x'`)
//! and `same-file-url-binding-v1` (a bare identifier standing as the WHOLE url argument, `fetch(URL)`).
//!
//! Why this lives beside [`super::consts`] instead of inside it: the project-wide constant map is
//! deliberately DOTTED-only (`ControlKey.AUTHEN.get`) because it is scope-INSENSITIVE — a bare common name
//! (`path`, `url`, `base`) folded into a project-wide, last-write-wins map could shadow a same-named
//! function parameter in an unrelated file and mis-key that call, which is a guess wearing a visible
//! fact's clothes. Measured, not theorized: mono-hub holds thirteen files each declaring `const URL` with
//! a DIFFERENT third-party origin, all in one tree. This map takes the opposite trade: bare names ARE
//! admitted, but only from ONE file's own AST and only through [`super::binding_census`], which proves the
//! name is bound exactly once in that file. The value at the call site is not inferred, it is read.
//!
//! **Two maps, because the two positions ask different questions.**
//!
//! - [`LocalConsts::head_literal_for`] answers the HEAD slot and admits only a plain string literal
//!   initializer. An interpolation in the MIDDLE of a path (`` `/users/${id}` ``) is a route parameter and
//!   `{}` is its correct normalization; substituting there would stop the call joining the `:param` route
//!   it belongs to. The head is the one slot where `{}` is not a normalization but a LOSS, because
//!   `consume_key_for`'s base-carrier head-drop has to ASSUME the opaque head carries no scheme or host —
//!   so `` fetch(`${BASE}/x`) `` with `const BASE = "https://v2.jokeapi.dev/joke"` files a third-party
//!   call as an internal route. When the base is visible in this file we do not have to assume.
//! - [`LocalConsts::whole_url_for`] answers the WHOLE-ARGUMENT position (`fetch(URL)`, `fetch(url, {…})`)
//!   and admits any initializer that RESOLVES ON ITS OWN through
//!   [`resolve_url_variants`](super::url_resolve) — a literal, a template, a `+`-chain, a two-literal
//!   ternary. There is no head-drop bucket in this position and nothing to assume: substituting is exactly
//!   equivalent to having found that expression written at the call site, which is what the file says.
//!   This is where `const url = …; fetch(url)` is resolved, and it subsumes the plain
//!   `const URL = "https://…"; fetch(URL)` case as the sub-case where the initializer is a literal.
//!
//! **Exactly one hop, and only when the initializer resolves by itself.** The `urls` map is built by
//! resolving each candidate's initializer against the `heads` map and an EMPTY `urls` map, so
//! `const a = "/x"; const b = a; fetch(b)` stops at `b`. Interprocedural resolution is out — a wrapper
//! parameter (`fetchJson(url)`) or a component prop is VALUE resolution, a different body of work with its
//! own quality bar, not constant reading.
//!
//! **Scope is deliberately ONE file.** A cross-file constant, an env binding (`process.env.X`), or a
//! deployment-supplied base is out of reach by construction and stays unresolved — a base value that is
//! itself an environment fact enters by injection, not inference.

use std::collections::HashMap;

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{Callee, Expr, Lit, Module, Tpl};

use super::binding_census::unambiguous_bindings;
use super::unwrap_expr;
use super::url_resolve::resolve_url_variants;

/// One file's same-file URL knowledge. Empty for the overwhelming majority of files; built once per file
/// by [`super::collector::extract_http_egress`].
///
/// Four maps, not two: each of the two POSITIONS above is answered separately for a bare NAME (`BASE`)
/// and for a zero-argument CALL (`base()`, `same-file-fn-url-v1`). The two namespaces stay apart on
/// purpose — a value and a function of the same name are different bindings, and one map would let
/// either answer for the other.
#[derive(Default)]
pub(crate) struct LocalConsts {
    /// Bare name -> plain string literal, for the LEADING slot only.
    heads: HashMap<String, String>,
    /// Bare name -> the URL variants its initializer resolves to, for the WHOLE-ARGUMENT position.
    urls: HashMap<String, Vec<String>>,
    /// Zero-argument helper name -> plain string literal it returns, for the LEADING slot only.
    fn_heads: HashMap<String, String>,
    /// Zero-argument helper name -> the URL variants its return resolves to, whole-argument position.
    fn_urls: HashMap<String, Vec<String>>,
}

impl LocalConsts {
    /// Build all four maps from one file's AST. `consts` is the project-wide DOTTED map, consulted while
    /// resolving an initializer exactly as it is at a call site (so `const u = ControlKey.A.b` resolves).
    pub(crate) fn build(module: &Module, consts: &HashMap<String, String>, cm: &SourceMap) -> Self {
        let gated = unambiguous_bindings(module);
        if gated.consts.is_empty() && gated.fn_returns.is_empty() {
            return Self::default();
        }
        let literal_map = |xs: &[(String, Expr)]| -> HashMap<String, String> {
            xs.iter()
                .filter_map(|(name, init)| match init {
                    Expr::Lit(Lit::Str(s)) => Some((
                        name.clone(),
                        s.value.as_str().unwrap_or_default().to_string(),
                    )),
                    _ => None,
                })
                .filter(|(_, value)| admits_substitution(value))
                .collect()
        };
        // The one-hop bound, structural rather than a counter: initializers and helper returns are both
        // resolved against a map whose whole-argument halves (`urls`/`fn_urls`) are EMPTY, so no
        // candidate can be defined in terms of another candidate's whole-argument value.
        let one_hop = Self {
            heads: literal_map(&gated.consts),
            fn_heads: literal_map(&gated.fn_returns),
            urls: HashMap::new(),
            fn_urls: HashMap::new(),
        };
        let resolve_all = |xs: &[(String, Expr)]| -> HashMap<String, Vec<String>> {
            let mut out = HashMap::new();
            for (name, init) in xs {
                let variants = resolve_url_variants(init, consts, &one_hop, cm);
                if !variants.is_empty() && variants.iter().all(|v| admits_substitution(v)) {
                    out.insert(name.clone(), variants);
                }
            }
            out
        };
        let urls = resolve_all(&gated.consts);
        let fn_urls = resolve_all(&gated.fn_returns);
        Self {
            heads: one_hop.heads,
            fn_heads: one_hop.fn_heads,
            urls,
            fn_urls,
        }
    }

    /// The literal to substitute for a URL's LEADING slot, or `None` to leave that slot opaque.
    ///
    /// `rest` is the VISIBLE LITERAL TEXT that immediately follows the slot in the assembled URL (a
    /// template's second quasi; a `+`-chain's second operand when that operand resolves to a literal). It
    /// is the empty string when the next piece is dynamic — and that is a REFUSAL, not a detail.
    ///
    /// Reading the head moves the call out of `consume_key_for`'s base-carrier head-drop bucket and past
    /// the never-guess vetoes that bucket enforces (`keying.rs`'s `{}{}`-head, non-`/` suffix and
    /// `//`-host pins). Those vetoes exist because the head is OPAQUE; once it is read the string is fully
    /// determined, so most of them stop applying. Two harms survive, and both are refused: the `//` value
    /// at construction ([`admits_substitution`]), and the empty `rest` here — a dynamic next piece would
    /// glue the head's literal text straight onto a `{}` (`` `${BASE}${path}` `` with `BASE = "/api"` ->
    /// `"/api{}"`). In this key vocabulary `{}` is ONE WHOLE path-param segment, so `/api{}` asserts a
    /// segment literally spelled `api<something>` — a route nobody wrote, and one that passes
    /// `key_carries_route_identity`, so it would be filed as an `unprovidedConsumes` drift finding rather
    /// than the honest silence it was before.
    ///
    /// A value that ENDS with `/` is deliberately NOT refused. `const B = "https://x.com/"` with
    /// `` `${B}/users` `` keys the external `GET https://x.com//users`, which is the literal truth of what
    /// the call requests and costs at most a duplicate spelling inside the external bucket (which never
    /// joins). Refusing would fall back to head-drop and key `GET /users` — a FALSE INTERNAL claim that can
    /// join a same-named local route. Between a cosmetic double slash and a wrong edge, the double slash is
    /// the lesser harm.
    ///
    /// A zero-argument call in this slot (`` `${base()}/charges` ``, `base() + '/charges'`) is answered
    /// from the helper map instead (`same-file-fn-url-v1`). Same gate, same refusals, same reason: the
    /// head is the one slot where `{}` is a LOSS rather than a normalization, and a helper returning
    /// `https://api.vendor.com` left the call filed as the internal route `GET /charges`.
    /// A bare NAME's plain string-literal value, if this file binds it exactly once to one.
    ///
    /// `pub(crate)` for [`crate::adapters::client_base`], which faces the same question in a
    /// different position: `axios.defaults.baseURL = API_BASE` needs the value behind `API_BASE`, and
    /// the gates that make that readable — bound once in this file, never reassigned, no parameter
    /// shadow, plain literal initializer — are exactly the ones this map already enforces. Sharing
    /// the map rather than re-collecting is deliberate: this crate had grown two other top-level
    /// const walks by 2026-08-08, and a third with its own slightly-different gates is how the
    /// "reads the value" and "proves the value is readable" halves drift apart.
    ///
    /// Takes a NAME rather than an `Expr` because the caller's position has no `rest` to gate on —
    /// [`Self::head_literal_for`] refuses an empty `rest` since a head with nothing after it is not a
    /// head, which is a rule about URL slots and says nothing about a base-URL assignment.
    pub(crate) fn literal_for_name(&self, name: &str) -> Option<&str> {
        self.heads.get(name).map(String::as_str)
    }

    pub(super) fn head_literal_for(&self, e: &Expr, rest: &str) -> Option<&str> {
        if rest.is_empty() {
            return None;
        }
        match unwrap_expr(e) {
            Expr::Ident(id) => self.heads.get(id.sym.as_ref()),
            other => zero_arg_callee_name(other).and_then(|n| self.fn_heads.get(n)),
        }
        .map(String::as_str)
    }

    /// The URL variants to substitute for a bare identifier that IS the whole url argument
    /// (`same-file-url-binding-v1`), or `None` to leave the call unresolved.
    ///
    /// Only a bare identifier, or a zero-argument call on one (`fetch(chargesUrl())`,
    /// `same-file-fn-url-v1`): a dotted or computed member (`ENDPOINT[kind]`) is the project-wide map's
    /// business, and a computed one stays unresolved rather than enumerating a value nobody wrote. A call
    /// WITH arguments is never answered — the returned string depends on what was passed, which is value
    /// resolution, not constant reading.
    pub(super) fn whole_url_for(&self, e: &Expr) -> Option<&[String]> {
        match unwrap_expr(e) {
            Expr::Ident(id) => self.urls.get(id.sym.as_ref()),
            other => zero_arg_callee_name(other).and_then(|n| self.fn_urls.get(n)),
        }
        .map(Vec::as_slice)
    }
}

/// The bare name a ZERO-ARGUMENT call invokes (`base()` -> `"base"`), or `None` for anything else — a
/// call with arguments, a method call (`api.base()`), or a non-call expression. The single spelling of
/// `same-file-fn-url-v1`'s call shape, shared by both positions above and by
/// [`resolve_url_variants`](super::url_resolve)'s call arm.
pub(super) fn zero_arg_callee_name(e: &Expr) -> Option<&str> {
    let Expr::Call(c) = unwrap_expr(e) else {
        return None;
    };
    if !c.args.is_empty() {
        return None;
    }
    let Callee::Expr(callee) = &c.callee else {
        return None;
    };
    match unwrap_expr(callee) {
        Expr::Ident(id) => Some(id.sym.as_ref()),
        _ => None,
    }
}

/// May this value stand in for the name at a URL position? Refused for a protocol-relative base
/// (`const CDN = "//cdn.example.com"`), which is the one harm that survives BOTH substitution positions:
/// the assembled `"//cdn.example.com/x"` is `/`-headed, so it keys internal and
/// [`normalize_http_path`](zzop_core::normalize_http_path) collapses the `//`, turning a third-party HOST
/// into an internal path SEGMENT (`GET /cdn.example.com/x`). That a literal `axios.get("//cdn/x")` already
/// keys that way is a pre-existing gap in the `/`-headed branch; this refusal keeps same-file constants
/// from widening the path into it. Applied once, at map construction, so the head slot and the
/// whole-argument position cannot answer it differently.
fn admits_substitution(value: &str) -> bool {
    !value.starts_with("//")
}

/// A template literal's `i`-th quasi as plain text (empty when absent or uncooked).
pub(super) fn quasi_text(t: &Tpl, i: usize) -> &str {
    t.quasis
        .get(i)
        .and_then(|q| q.cooked.as_ref())
        .and_then(|a| a.as_str())
        .unwrap_or_default()
}

/// True when a template literal STARTS with its first interpolation (`` `${BASE}/x` ``) rather than with
/// quasi text (`` `/api/${v}` ``) — the positional half of the head-only rule above.
pub(super) fn tpl_starts_with_interpolation(t: &Tpl) -> bool {
    !t.exprs.is_empty() && quasi_text(t, 0).is_empty()
}
