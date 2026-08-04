//! Spring `RestTemplate`/`WebClient` literal HTTP egress CONSUMES — the CONSUME-side counterpart of
//! `provides`, closing the half of the cross-layer join this crate's `FRAMEWORK_RECOGNIZERS` note
//! disclosed as absent (a Java service that CALLS another service contributed nothing to the consume
//! side). Mirrors `zzop_parser_csharp::adapters::http_clients`'s producer shape and `IoConsume` field
//! conventions exactly.
//!
//! - **Import-gate**: only extract when the file's own imports (`lang::imports::parse_imports`) bind
//!   [`RESTTEMPLATE_SPECIFIERS`] / [`WEBCLIENT_SPECIFIERS`] — the exact class FQN or its package glob.
//!   Each vocabulary is gated by ITS OWN import, so a `WebClient`-only file never runs the
//!   `RestTemplate` matcher and vice versa.
//! - **`RestTemplate` call shapes**: the CLIENT-SPECIFIC method names in [`RESTTEMPLATE_VERB_METHODS`]
//!   (`getForObject`, `postForEntity`, ...) on ANY receiver — the same "no type inference, import gate +
//!   method-NAME vocabulary" scope the C# `HttpClient` recognizer accepts, safe for the same reason:
//!   these names exist on no other common type. `exchange(url, HttpMethod.X, ...)` and
//!   `execute(url, HttpMethod.X, ...)` (the callback-taking sibling every `*ForObject` convenience
//!   method delegates to — `execute` is a RestTemplate-specific name, so no `Map.put`-class false key)
//!   are recognized only
//!   when the SECOND argument is a literal `HttpMethod.<VERB>` field access — a variable/computed method
//!   skips the whole call (never guessed); an `execute` with fewer than two arguments (e.g.
//!   `Executor.execute(runnable)`) is skipped the same way. The generic-named `put`/`delete` methods are deliberately NOT
//!   recognized: `java.util.Map::put` (and every collection's `put`/`delete`-shaped sibling) would
//!   false-key in any `RestTemplate`-importing file, the exact "any receiver" net an opus review (F1)
//!   made `zzop_parser_rust` retract — a real PUT/DELETE egress is silently under-reported instead of a
//!   spurious one fabricated. Recognizing them would need a receiver-binding pass (the Rust shape);
//!   roadmap, not attempted.
//! - **`WebClient` call shapes**: a `.uri(<arg>)` invocation whose receiver CHAIN contains a zero-arg
//!   verb call (`.get()`, `.post()`, `.put()`, `.patch()`, `.delete()` — `zzop_core::HTTP_KEY_VERBS`
//!   lowercased) or `.method(HttpMethod.X)`. A `.uri()` with no such verb upstream (a `UriBuilder`, an
//!   unrelated builder) is skipped entirely.
//! - **URL resolution**: a plain string literal only (`util::string_literal_text`) — Java has no string
//!   interpolation, so there is no template-reassembly arm; concatenation (`"/api/" + id`), a constant
//!   reference, or a lambda `.uri(b -> ...)` is unresolved, never guessed.
//! - **Keying** (mirrors `consume_key_for` in `zzop_parser_csharp::adapters::http_clients` exactly): a
//!   `/`-headed resolved URL -> `http_consume_interface_key` (drops any `?...`/`#...` suffix); an
//!   absolute `http(s)://` URL -> `"METHOD <url>"` verbatim; anything else (a base-relative literal —
//!   common under `WebClient.builder().baseUrl(...)`) -> unresolved:
//!   `IoConsume { key: None, raw: Some(<verbatim source>), method: Some(<VERB>), ... }` — witnessed,
//!   never guessed. A call with no argument at all is skipped. `client` is `Some("resttemplate")` /
//!   `Some("webclient")` either way.
//! - **Test surface is excluded by PATH** (`zzop_core::is_test_file`): Java tests live in their own
//!   source root (`src/test/java/**` — the `/test/` segment matches) or name themselves
//!   (`*Test.java`/`*Tests.java`/`Test*.java` — all matched), and Java has NO inline in-source test
//!   idiom (no `#[cfg(test)]` analogue; a JUnit test is always a separate class in a separate path), so
//!   the path gate alone is sufficient — no attribute/subtree gate like `zzop_parser_rust`'s is built,
//!   deliberately.
//! - **Deliberately NOT recognized** (disclosed, mirrors this crate's recognizer-note discipline):
//!   Feign (`@FeignClient` declarative interfaces — the paths are Spring mapping annotations this crate
//!   already parses, but the consume belongs to the TARGET service named by the client's `name`/`url`
//!   attribute, which needs its own adapter and a base-URL story; roadmap) and `java.net.http.HttpClient`
//!   (the URL lives on a separately built `HttpRequest`, not at the `send` call site — the same "not
//!   visible at the call site" reason C# skips `SendAsync`).

use tree_sitter::Node;
use zzop_core::{http_consume_interface_key, IoConsume, HTTP_KEY_VERBS};

use crate::util::{line_of, node_text, string_literal_text, valid_named_children};

/// `RestTemplate` client-specific method name -> emitted key verb — see module doc's call-shape section
/// (`exchange`/`execute` are handled separately; `put`/`delete` deliberately absent).
const RESTTEMPLATE_VERB_METHODS: &[(&str, &str)] = &[
    ("getForObject", "GET"),
    ("getForEntity", "GET"),
    ("postForObject", "POST"),
    ("postForEntity", "POST"),
    ("postForLocation", "POST"),
    ("patchForObject", "PATCH"),
];

const RESTTEMPLATE_SPECIFIERS: &[&str] = &[
    "org.springframework.web.client.RestTemplate",
    "org.springframework.web.client.*",
];

const WEBCLIENT_SPECIFIERS: &[&str] = &[
    "org.springframework.web.reactive.function.client.WebClient",
    "org.springframework.web.reactive.function.client.*",
];

/// Extract this file's `RestTemplate`/`WebClient` HTTP egress consumes — see module doc. Empty on parse
/// failure, on a test-classified path, and whenever the file imports neither client (never panics).
pub fn extract_java_http_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    // Test surface — see the module doc's path-gate judgment. Path first (no parse needed).
    if zzop_core::is_test_file(rel) {
        return Vec::new();
    }
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    let gate = |specs: &[&str]| {
        imports
            .values()
            .any(|b| specs.contains(&b.specifier.as_str()))
    };
    let resttemplate = gate(RESTTEMPLATE_SPECIFIERS);
    let webclient = gate(WEBCLIENT_SPECIFIERS);
    if !resttemplate && !webclient {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk(
        tree.root_node(),
        rel,
        text,
        resttemplate,
        webclient,
        &mut out,
    );
    out
}

fn walk(
    node: Node,
    rel: &str,
    src: &str,
    resttemplate: bool,
    webclient: bool,
    out: &mut Vec<IoConsume>,
) {
    if node.kind() == "method_invocation" {
        if resttemplate {
            if let Some((verb, url_arg)) = match_resttemplate_call(node, src) {
                out.push(emit(rel, verb, url_arg, src, "resttemplate"));
            }
        }
        if webclient {
            if let Some((verb, url_arg)) = match_webclient_uri_call(node, src) {
                out.push(emit(rel, verb, url_arg, src, "webclient"));
            }
        }
    }
    for child in valid_named_children(node) {
        walk(child, rel, src, resttemplate, webclient, out);
    }
}

/// `<any receiver>.<RestTemplate-specific method>(url, ...)` -> `(VERB, url argument node)`; for
/// `exchange`/`execute`, the verb comes from a literal `HttpMethod.X` SECOND argument or the call is
/// skipped — which also skips `Executor.execute(runnable)` (one argument, no second to read) without
/// any receiver knowledge.
fn match_resttemplate_call<'t>(call: Node<'t>, src: &str) -> Option<(&'static str, Node<'t>)> {
    let name = call.child_by_field_name("name")?;
    let method_name = node_text(name, src);
    let args = call.child_by_field_name("arguments")?;
    let arg_nodes = valid_named_children(args);
    let url_arg = *arg_nodes.first()?;
    if let Some((_, verb)) = RESTTEMPLATE_VERB_METHODS
        .iter()
        .find(|(m, _)| *m == method_name)
    {
        return Some((verb, url_arg));
    }
    if method_name == "exchange" || method_name == "execute" {
        let verb = http_method_field_verb(*arg_nodes.get(1)?, src)?;
        return Some((verb, url_arg));
    }
    None
}

/// `<chain containing .get()/.post()/…/.method(HttpMethod.X)>.uri(<arg>)` -> `(VERB, uri argument)`.
fn match_webclient_uri_call<'t>(call: Node<'t>, src: &str) -> Option<(&'static str, Node<'t>)> {
    let name = call.child_by_field_name("name")?;
    if node_text(name, src) != "uri" {
        return None;
    }
    let args = call.child_by_field_name("arguments")?;
    let url_arg = valid_named_children(args).into_iter().next()?;
    let verb = chain_verb(call, src)?;
    Some((verb, url_arg))
}

/// The verb named by the nearest verb-shaped call in `call`'s receiver chain: a ZERO-arg
/// `.get()/.post()/.put()/.patch()/.delete()`, or `.method(HttpMethod.X)` with a literal field access.
/// `None` when the chain carries no such call (not a recognized `WebClient` request builder).
fn chain_verb(call: Node, src: &str) -> Option<&'static str> {
    let mut cur = call.child_by_field_name("object");
    while let Some(n) = cur {
        if n.kind() != "method_invocation" {
            return None;
        }
        if let Some(name) = n.child_by_field_name("name") {
            let m = node_text(name, src);
            let arg_nodes = n
                .child_by_field_name("arguments")
                .map(valid_named_children)
                .unwrap_or_default();
            if arg_nodes.is_empty() {
                if let Some(verb) = HTTP_KEY_VERBS.iter().find(|v| v.eq_ignore_ascii_case(m)) {
                    return Some(verb);
                }
            }
            if m == "method" && arg_nodes.len() == 1 {
                return http_method_field_verb(arg_nodes[0], src);
            }
        }
        cur = n.child_by_field_name("object");
    }
    None
}

/// A literal `HttpMethod.<VERB>` field access -> the verb, for `exchange`/`execute`'s second argument and
/// `WebClient.method(...)` — `None` for any other expression shape (never guessed).
fn http_method_field_verb(node: Node, src: &str) -> Option<&'static str> {
    if node.kind() != "field_access" {
        return None;
    }
    let object = node.child_by_field_name("object")?;
    if node_text(object, src) != "HttpMethod" {
        return None;
    }
    let field = node.child_by_field_name("field")?;
    let name = node_text(field, src);
    HTTP_KEY_VERBS.iter().find(|v| **v == name).copied()
}

fn emit(rel: &str, verb: &str, url_arg: Node, src: &str, client: &str) -> IoConsume {
    let resolved = string_literal_text(url_arg, src);
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
        client: Some(client.to_string()),
    }
}

/// Mirrors `zzop_parser_csharp::adapters::http_clients::consume_key_for` exactly.
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
