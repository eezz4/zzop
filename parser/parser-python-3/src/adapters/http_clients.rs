//! `requests`/`httpx` literal HTTP egress CONSUMES — the consume-side counterpart of `adapters::fastapi`'s
//! route PROVIDES, at the same v1-scope discipline: import-gated (a file that never imports `requests` or
//! `httpx` yields nothing), TOP-LEVEL-call-shape recognition only (`<module>.<verb>(<url>, ...)`, the
//! module's own top-level segment binding per `lang::imports`' "plain `import a.b.c` binds only the
//! top-level segment `a`" rule — so `import requests` binds `requests`, matching `requests.get(...)`).
//!
//! - **Verb methods**: `get`/`post`/`put`/`patch`/`delete` — the same five `adapters::fastapi::
//!   VERB_DECORATORS` recognizes, kept as a separate vocabulary (this one names CALLED methods, that one
//!   DECORATOR names) since the two crates' conventions happen to agree one-for-one for these five.
//! - **Instance receivers** (`requests.Session()`, `httpx.Client()`/`AsyncClient()`): a `LAST-WRITE-WINS`
//!   pass (`instances::instance_bindings`, backlog B14①'s fix for the flat approximation the
//!   `zzop_parser_rust::adapters::http_clients::BindingCollector` still carries) binds a local name to a
//!   client CONSTRUCTOR — `<module>.Session/Client/AsyncClient(...)` (module in `client_names`) or a
//!   directly-imported `Session/Client/AsyncClient(...)` (`ctor_direct_names`) — through an assignment
//!   (`s = requests.Session()`, `client = httpx.AsyncClient()`, incl. the annotated `client:
//!   httpx.AsyncClient = ...`) or a `with`/`async with` binding (`async with httpx.AsyncClient() as
//!   client:` — the idiomatic FastAPI async-egress shape), and resolves a `.verb(...)` call against that
//!   name's MOST RECENT binding at or before the call's own line — see `instances`' module doc for the
//!   full resolution contract, kill classes (a non-client reassignment or `del`), and the remaining
//!   file-level (not per-function-scope) grain this still carries. `.request(method, url)`
//!   (verb-as-first-arg) stays out of v1 scope.
//! - **Call-site discovery**: a generic `ruff_python_ast::visitor::Visitor` walk (mirroring
//!   `lang::used_names::parse_local_identifier_refs`'s use of the same crate visitor) rather than a
//!   hand-rolled statement/expression descent — every expression position (nested call args, dict/list
//!   elements, keyword-arg values, a `with`-statement's context expression, ...) is visited automatically,
//!   so `requests.get(url).json()`, `{"r": requests.get(url)}`, `requests.get(url=path)`, and
//!   `with requests.get(url) as r:` are all found, not just a top-level call's own positional args.
//! - **Keying** (`consume-key-discipline-v1`, mirroring `zzop_parser_typescript::adapters::egress`'s
//!   `consume_key_for` exactly for the two shapes below — see that function's doc for the full TS-side
//!   bucket list this deliberately narrows):
//!   - A `/`-headed resolved URL string -> `IoConsume{kind: "http", key: Some(zzop_core::
//!     http_consume_interface_key(method, url)), ...}` (drops any `?...`/`#...` query/fragment suffix
//!     before normalizing — the consume-side key discipline).
//!   - An absolute `http(s)://` resolved URL string -> `IoConsume{key: Some("METHOD <url>"), ...}`
//!     (host-carrying, verbatim, not run through `http_consume_interface_key` which would mangle the
//!     origin) — same external classification `egress::consume_key_for` uses.
//!   - Any other resolved URL string (no leading `/`, not `http(s)://` — e.g. a base-relative literal
//!     `"users/login"`) -> unresolved, same as an unresolvable expression. Unlike `egress`, this
//!     deliberately does NOT implement TS's base-relative-path bucket (no `requests`/`httpx` `baseURL`-
//!     indirection idiom is evidenced the way axios/ky's `baseURL` config is) — never invent evidence this
//!     adapter's own call sites don't share.
//!   - A "resolved URL string" comes from a string literal verbatim, OR an f-string reassembled by
//!     replacing every interpolation with `{}` and concatenating the literal parts in order (e.g.
//!     `f"/users/{uid}"` -> `"/users/{}"`, then keyed like any other `/`-headed literal). A `{}`-HEADED
//!     reassembled string — one whose first element is an interpolation, so there is no literal path head
//!     at all (`f"{base}/users"` -> `"{}/users"`) — is deliberately left unresolved even though it starts
//!     with neither `/` nor a scheme: unlike TS's template/const-map indirection, there is no cross-file
//!     constant this adapter could have resolved `base` against, so guessing "internal, rooted" from a
//!     bare `{}` head would be inventing a fact never written down. An implicitly-concatenated f-string
//!     (`"a" f"b {x}"`) is out of v1 scope (unresolved, same as any other unrecognized shape).
//! - **Unresolved**: any other first-positional-argument shape (a bare name, a binary-op concatenation, a
//!   nested call, an f-string headed by an interpolation, a resolved-but-unkeyable URL string, ...) ->
//!   `IoConsume{kind: "http", key: None, raw: Some(<raw source text>), method: Some(<UPPERCASE verb>),
//!   ...}` — never guessed at, but still witnessed (the adapter SAW the call site, just could not resolve
//!   its target statically), same "still counted, not silently dropped" contract
//!   `zzop_parser_typescript::adapters::egress` upholds for its own non-literal call sites.
//! - A call with no positional argument at all is skipped entirely (nothing to key, nothing to report).

use std::collections::HashSet;

use ruff_python_ast::visitor::{walk_expr, Visitor};
use ruff_python_ast::{Expr, InterpolatedStringElement};
use ruff_text_size::Ranged;
use zzop_core::{http_consume_interface_key, ImportMap, IoConsume};

mod instances;

pub(crate) const VERB_METHODS: &[&str] = &["get", "post", "put", "patch", "delete"];

/// Extract this file's `requests`/`httpx` HTTP egress consumes — see module doc. Empty on parse failure,
/// and whenever the file imports neither `requests` nor `httpx` (never panics).
pub fn extract_python_http_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    let client_names = http_client_receiver_names(&imports);
    if client_names.is_empty() {
        return Vec::new();
    }
    let idx = crate::LineIndex::new(text);
    // File-wide last-write-wins pass: for every name ever bound to a client constructor, a binding
    // history keyed by line — a `.verb()` call resolves against the MOST RECENT binding at or before its
    // own line (module doc's "Instance receivers"; full resolution contract in `instances`' module doc).
    let instances = instances::instance_bindings(&module.body, &client_names, &imports, &idx);
    let mut collector = CallCollector {
        rel,
        text,
        idx: &idx,
        client_names: &client_names,
        instances: &instances,
        out: Vec::new(),
    };
    for stmt in &module.body {
        collector.visit_stmt(stmt);
    }
    collector.out
}

/// Local names bound to a `requests`/`httpx` import (top-level-segment binding, or a direct `as` alias —
/// see `lang::imports`' own doc for the exact binding rule a plain `import requests`/`import requests as
/// r` follows).
fn http_client_receiver_names(imports: &ImportMap) -> HashSet<String> {
    imports
        .iter()
        .filter(|(_, b)| {
            b.specifier == "requests"
                || b.specifier == "httpx"
                || b.specifier.starts_with("requests.")
                || b.specifier.starts_with("httpx.")
        })
        .map(|(local, _)| local.clone())
        .collect()
}

/// Generic-visitor call-site collector (`ruff_python_ast::visitor::Visitor`, the same pattern
/// `lang::used_names::RefCollector` uses) — replaces a hand-rolled statement/expression descent so every
/// expression position is visited, not just the shapes a manual walk happened to enumerate (module doc).
struct CallCollector<'a> {
    rel: &'a str,
    text: &'a str,
    idx: &'a crate::LineIndex,
    client_names: &'a HashSet<String>,
    instances: &'a instances::Bindings,
    out: Vec<IoConsume>,
}

impl<'a> Visitor<'a> for CallCollector<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            if let Some((method, url_arg)) = self.match_http_call(call) {
                self.emit(method, url_arg);
            }
        }
        // Keep descending regardless — a call can nest another call (`requests.get(url).json()`), sit
        // inside a dict/list literal, a keyword argument, a `with` context expression, ... — the generic
        // walk covers every one of those positions without this adapter special-casing any of them.
        walk_expr(self, expr);
    }
}

impl<'a> CallCollector<'a> {
    /// `<client>.<verb>(...)` where `<client>` is one of `client_names` and `<verb>` is a recognized
    /// method — returns the UPPERCASE method and the first positional argument expr, if any.
    fn match_http_call(&self, call: &'a ruff_python_ast::ExprCall) -> Option<(String, &'a Expr)> {
        let Expr::Attribute(attr) = &*call.func else {
            return None;
        };
        let Expr::Name(recv) = &*attr.value else {
            return None;
        };
        // A top-level module call (`requests.get(...)`) OR a bound client instance (`client.get(...)`,
        // `s.get(...)` — resolved against `recv`'s own line via `instances::Bindings::is_client_at`'s
        // last-write-wins contract). Both key identically.
        let recv_line = self.idx.line_of(attr.value.start());
        if !self.client_names.contains(recv.id.as_str())
            && !self.instances.is_client_at(recv.id.as_str(), recv_line)
        {
            return None;
        }
        let verb = attr.attr.as_str();
        if !VERB_METHODS.contains(&verb) {
            return None;
        }
        let url_arg = call.arguments.find_positional(0)?;
        Some((verb.to_ascii_uppercase(), url_arg))
    }

    fn emit(&mut self, method: String, url_arg: &Expr) {
        let line = self.idx.line_of(url_arg.start());
        if let Some(resolved) = resolved_url_literal(url_arg) {
            if let Some(key) = consume_key_for(&method, &resolved) {
                self.out.push(IoConsume {
                    kind: "http".to_string(),
                    key: Some(key),
                    file: self.rel.to_string(),
                    line,
                    raw: None,
                    method: None,
                    retry_configured: None,
                    body: None,
                    client: None,
                });
                return;
            }
        }
        // Unresolved: never partially assembled, never guessed — still witnessed.
        let raw = self.raw_text(url_arg);
        self.out.push(IoConsume {
            kind: "http".to_string(),
            key: None,
            file: self.rel.to_string(),
            line,
            raw: Some(raw),
            method: Some(method),
            retry_configured: None,
            body: None,
            client: None,
        });
    }

    /// The verbatim source text spanning `expr`'s range — `IoConsume::raw`'s provenance string for an
    /// unresolved call site.
    fn raw_text(&self, expr: &Expr) -> String {
        let start = usize::from(expr.start());
        let end = usize::from(expr.end());
        self.text.get(start..end).unwrap_or_default().to_string()
    }
}

/// Resolve a URL argument expression to a literal path string, if statically knowable (module doc): a
/// plain string literal verbatim, or a single-part f-string reassembled by replacing every
/// `InterpolatedStringElement::Interpolation` with `{}` and concatenating the literal elements in order.
/// An implicitly-concatenated f-string (`"a" f"b {x}"`) and any other expression shape (bare name,
/// binary-op concatenation, nested call, ...) yield `None` — never guessed.
fn resolved_url_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::StringLiteral(s) => Some(s.value.to_str().to_string()),
        Expr::FString(f) => {
            if f.value.is_implicit_concatenated() {
                return None;
            }
            let mut out = String::new();
            for el in f.value.elements() {
                match el {
                    InterpolatedStringElement::Literal(lit) => out.push_str(&lit.value),
                    InterpolatedStringElement::Interpolation(_) => out.push_str("{}"),
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// Mirrors `zzop_parser_typescript::adapters::egress::consume_key_for`'s discipline for the two shapes
/// this adapter recognizes — see module doc's "Keying" section for the full explanation and why the
/// base-relative-path bucket is deliberately not ported.
fn consume_key_for(method: &str, url: &str) -> Option<String> {
    if url.starts_with('/') {
        Some(http_consume_interface_key(method, url))
    } else if is_external(url) {
        Some(format!("{} {}", method.to_uppercase(), url))
    } else {
        None
    }
}

fn is_external(u: &str) -> bool {
    let l = u.to_ascii_lowercase();
    l.starts_with("http://") || l.starts_with("https://")
}

#[cfg(test)]
mod tests;
