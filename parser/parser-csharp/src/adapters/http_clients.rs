//! `HttpClient` literal HTTP egress CONSUMES — the CONSUME-side counterpart of `adapters::provides`,
//! mirroring `zzop_parser_go::adapters::http_clients`'s producer shape and `IoConsume` field
//! conventions exactly.
//!
//! - **Import-gate**: only extract when the file's own `using` set (`lang::imports::parse_imports`)
//!   binds a specifier of `System.Net.Http` or `System.Net.Http.Json` — mirrors Go's `net/http`
//!   import-bound-name gate, simplified for C#: since `HttpClient` is a TYPE (not a namespace alias
//!   Go's free functions hang off), this crate does not track the receiver's own declared/inferred
//!   type — recognition is import-gate + method-NAME vocabulary only (`VERB_METHODS` below), the same
//!   "no full type inference, name-vocabulary + import gate" scope Go's own `net/http` recognizer
//!   accepts for ITS free functions. Documented v1 approximation: a same-named method on an unrelated
//!   type (rare in practice, since `GetAsync`/`PostAsJsonAsync`/... are `HttpClient`-specific method
//!   names) would also match once the file imports `System.Net.Http` — accepted, never guessed beyond
//!   that.
//! - **Call shapes**: `.GetAsync`, `.PostAsync`, `.PutAsync`, `.DeleteAsync`, `.PatchAsync`,
//!   `.GetStringAsync`, `.GetByteArrayAsync`, `.GetStreamAsync`, `.GetFromJsonAsync`,
//!   `.PostAsJsonAsync`, `.PutAsJsonAsync`, `.DeleteFromJsonAsync` on ANY receiver — `url` is always
//!   the FIRST positional argument. `.SendAsync` is deliberately NOT recognized (needs a separately
//!   constructed `HttpRequestMessage` — roadmap, same "not visible at the call site" note Go's own doc
//!   carries for its `*http.Client`/`client.Do(req)` skip).
//! - **URL resolution**: a plain string literal verbatim, or an interpolated string
//!   (`$"...{expr}..."`) with every `{expr}` hole collapsed to `{}` — the C# analogue of Go's
//!   `fmt.Sprintf` template reassembly, except the grammar ALREADY segments each interpolation hole as
//!   its own `interpolation` node, so no `%`-verb regex scan is needed at all (`normalize_interpolated`
//!   below).
//! - **Keying** (mirrors `consume_key_for` in `zzop_parser_go::adapters::http_clients` exactly): a
//!   `/`-headed resolved URL -> `http_consume_interface_key` (drops any `?...`/`#...` suffix); an
//!   absolute `http(s)://` resolved URL -> `"METHOD <url>"` verbatim; anything else (a base-relative
//!   literal, or a `{}`-HEADED reassembled template with no literal path head at all) -> unresolved.
//! - **Unresolved**: any other first-argument shape (a bare identifier, string concatenation, a nested
//!   non-string-literal expression, a resolved-but-unkeyable literal, ...) ->
//!   `IoConsume { key: None, raw: Some(<verbatim source text>), method: Some(<verb>), ... }` — witnessed,
//!   never guessed. A call with no positional argument at all is skipped entirely. `client` is always
//!   `Some("httpclient")`, resolved or not — the receiver IS a recognized `HttpClient`-method call site
//!   either way.
//! - Call-site discovery is a FULL CST walk (nested call sites reachable), mirroring
//!   `zzop_parser_go::adapters::http_clients`'s identical full-walk scope.

use tree_sitter::Node;
use zzop_core::{http_consume_interface_key, IoConsume};

use crate::util::{line_of, node_text, string_literal_text, valid_named_children};

/// `HttpClient` method name -> emitted key verb — see module doc's "call shapes" section.
const VERB_METHODS: &[(&str, &str)] = &[
    ("GetAsync", "GET"),
    ("PostAsync", "POST"),
    ("PutAsync", "PUT"),
    ("DeleteAsync", "DELETE"),
    ("PatchAsync", "PATCH"),
    ("GetStringAsync", "GET"),
    ("GetByteArrayAsync", "GET"),
    ("GetStreamAsync", "GET"),
    ("GetFromJsonAsync", "GET"),
    ("PostAsJsonAsync", "POST"),
    ("PutAsJsonAsync", "PUT"),
    ("DeleteFromJsonAsync", "DELETE"),
];

const HTTP_CLIENT_SPECIFIERS: &[&str] = &["System.Net.Http", "System.Net.Http.Json"];

/// Extract this file's `HttpClient` HTTP egress consumes — see module doc. Empty on parse failure, and
/// whenever the file does not import `System.Net.Http`/`System.Net.Http.Json` (never panics).
pub fn extract_csharp_http_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    let gated = imports
        .values()
        .any(|b| HTTP_CLIENT_SPECIFIERS.contains(&b.specifier.as_str()));
    if !gated {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk(tree.root_node(), rel, text, &mut out);
    out
}

fn walk(node: Node, rel: &str, src: &str, out: &mut Vec<IoConsume>) {
    if node.kind() == "invocation_expression" {
        if let Some((verb, url_arg)) = match_client_call(node, src) {
            out.push(emit(rel, verb, url_arg, src));
        }
    }
    for child in valid_named_children(node) {
        walk(child, rel, src, out);
    }
}

/// `<any receiver>.<Verb-shaped method name>(url, ...)` -> `(UPPERCASE method, url argument node)`.
fn match_client_call<'t>(call: Node<'t>, src: &str) -> Option<(&'static str, Node<'t>)> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "member_access_expression" {
        return None;
    }
    let name_node = func.child_by_field_name("name")?;
    let method_name = node_text(name_node, src);
    let (_, verb) = VERB_METHODS.iter().find(|(m, _)| *m == method_name)?;
    let args = call.child_by_field_name("arguments")?;
    let first = valid_named_children(args)
        .into_iter()
        .find(|a| a.kind() == "argument")?;
    let url_arg = valid_named_children(first).into_iter().next()?;
    Some((verb, url_arg))
}

fn emit(rel: &str, verb: &str, url_arg: Node, src: &str) -> IoConsume {
    let resolved = resolved_url(url_arg, src);
    let key = resolved.as_deref().and_then(|u| consume_key_for(verb, u));
    let (raw, method) = match &key {
        Some(_) => (None, None),
        None => (
            Some(node_text(url_arg, src).to_string()),
            Some(verb.to_string()),
        ),
    };
    IoConsume {
        kind: "http".to_string(),
        key,
        file: rel.to_string(),
        line: line_of(url_arg),
        raw,
        method,
        retry_configured: None,
        body: None,
        client: Some("httpclient".to_string()),
    }
}

/// A plain string literal verbatim, or an interpolated string's normalized template — module doc's "URL
/// resolution" section. `None` for any other expression shape (a bare identifier, concatenation,
/// nested call, ...) — never guessed.
fn resolved_url(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "string_literal" => string_literal_text(node, src),
        "interpolated_string_expression" => Some(normalize_interpolated(node, src)),
        _ => None,
    }
}

/// Collapses every `interpolation` hole to `{}`, keeping every `string_content` span verbatim — the C#
/// analogue of Go's `fmt.Sprintf` `%`-verb collapse, needed here only because the grammar already
/// segments holes as distinct nodes (module doc).
fn normalize_interpolated(node: Node, src: &str) -> String {
    let mut out = String::new();
    for child in valid_named_children(node) {
        match child.kind() {
            "string_content" => out.push_str(node_text(child, src)),
            "interpolation" => out.push_str("{}"),
            _ => {}
        }
    }
    out
}

/// Mirrors `zzop_parser_go::adapters::http_clients::consume_key_for` exactly.
fn consume_key_for(method: &str, url: &str) -> Option<String> {
    if url.starts_with('/') {
        Some(http_consume_interface_key(method, url))
    } else if is_external(url) {
        Some(format!("{method} {url}"))
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
