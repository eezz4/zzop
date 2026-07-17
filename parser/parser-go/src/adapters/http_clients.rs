//! `net/http` client literal HTTP egress CONSUMES — the CONSUME-side counterpart of
//! `adapters::net_http`'s route PROVIDES. Import-gated on `net/http`; a file that never imports it
//! yields nothing.
//!
//! - **Call shapes**: the package-level free functions `http.Get(url)`, `http.Post(url, contentType,
//!   body)`, `http.PostForm(url, data)`, `http.Head(url)` — `<net/http-name>.<Verb>(...)`, gated on the
//!   file's own `net/http` import-bound local name(s) (mirrors `zzop_parser_rust::adapters::
//!   http_clients`'s import-bound-receiver-name discipline, minus that crate's extra method-chain/
//!   binding-propagation rules 2/3 — those exist there because `reqwest::Client` VALUES get built and
//!   passed around; `net/http`'s free functions need no such tracking). `url` is always the FIRST
//!   positional argument for all four — `Post`/`PostForm`'s own extra parameters never carry the URL.
//!   `Head` maps to method `"HEAD"`, outside `zzop_core::HTTP_KEY_VERBS`'s five-verb vocabulary but
//!   still emitted verbatim: `net/http`'s own function NAME is an explicit verb spelling (like a
//!   Spring `method = RequestMethod.HEAD` attribute), not a name-shaped GUESS — `HTTP_KEY_VERBS`'s own
//!   doc explicitly carves out this "explicit attribute passes through verbatim" case.
//! - **`&http.Client{}`/`client.Do(req)` instances**: SKIPPED in v1 — a request built via
//!   `http.NewRequest(...)` and dispatched through a separately-constructed `*http.Client` value is not
//!   visible at the call site the way a free-function URL argument is (same "roadmap, not attempted"
//!   note `zzop_parser_python_3::adapters::http_clients`'s module doc leaves for a `requests.Session`/
//!   `httpx.Client` instance).
//! - **URL resolution**: a string literal verbatim, OR `fmt.Sprintf("template", args...)` (gated on
//!   the file's own `fmt` import) whose FIRST argument is a string literal with every `%`-verb
//!   (`%s`/`%d`/%v`/`%q`/a flag+width+precision cluster like `%05.2f`/an explicit arg index `%[1]s`)
//!   collapsed to `{}` — the Go analogue of `zzop_parser_rust::adapters::http_clients`'s `format!`
//!   reassembly and `zzop_parser_python_3::adapters::http_clients`'s f-string reassembly. A literal
//!   escaped `%%` is left as `%%` (Go's own escape, mirroring those two crates' `{{`/`}}` handling).
//! - **Keying** (mirrors `consume_key_for` in both sibling crates exactly): a `/`-headed resolved URL
//!   -> `zzop_core::http_consume_interface_key` (drops any `?...`/`#...` suffix); an absolute
//!   `http(s)://` resolved URL -> `"METHOD <url>"` verbatim; anything else (a base-relative literal, OR
//!   a `{}`-HEADED reassembled `Sprintf` template with no literal path head at all) -> unresolved.
//! - **Unresolved**: any other first-argument shape (a bare name, string concatenation via `+`, a
//!   nested non-`Sprintf` call, a resolved-but-unkeyable literal, ...) -> `IoConsume{key: None, raw:
//!   Some(<verbatim source text>), method: Some(<verb>), ...}` — witnessed, never guessed. A call with
//!   no positional argument at all is skipped entirely.
//! - Call-site discovery is a FULL CST walk (nested call sites reachable — module doc parity with
//!   `adapters::net_http`/`adapters::gin`, the F3 defect class the task brief calls out).

use std::collections::HashSet;

use tree_sitter::Node;
use zzop_core::{http_consume_interface_key, ImportMap, IoConsume};

use crate::util::{node_text, string_literal_text, valid_named_children};

use super::nth_arg;

/// `net/http` client helper name -> emitted key verb. Every verb is a `zzop_core::HTTP_KEY_VERBS`
/// member EXCEPT `HEAD` — a deliberate T3 divergence (do not unify): `http.Head(...)` is a real
/// client-side egress fact worth recording, but no provider-side extractor keys HEAD routes (the
/// core key vocabulary is the 5 join verbs), so a HEAD consume is honest-but-unjoinable by design
/// (it lands in unconsumed/external buckets, never edges). Pinned by
/// `verb_methods_verbs_are_core_key_verbs_plus_deliberate_head` below.
const VERB_METHODS: &[(&str, &str)] = &[
    ("Get", "GET"),
    ("Post", "POST"),
    ("PostForm", "POST"),
    ("Head", "HEAD"),
];

/// Extract this file's `net/http` client HTTP egress consumes — see module doc. Empty on parse
/// failure, and whenever the file does not import `net/http` (never panics).
pub fn extract_go_http_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    let net_http_names = local_names(&imports, "net/http");
    if net_http_names.is_empty() {
        return Vec::new();
    }
    let fmt_names = local_names(&imports, "fmt");
    let mut out = Vec::new();
    walk(
        tree.root_node(),
        rel,
        text,
        &net_http_names,
        &fmt_names,
        &mut out,
    );
    out
}

fn local_names(imports: &ImportMap, specifier: &str) -> HashSet<String> {
    imports
        .iter()
        .filter(|(_, b)| b.specifier == specifier)
        .map(|(local, _)| local.clone())
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn walk(
    node: Node,
    rel: &str,
    src: &str,
    net_http_names: &HashSet<String>,
    fmt_names: &HashSet<String>,
    out: &mut Vec<IoConsume>,
) {
    if node.is_error() || node.is_missing() {
        return;
    }
    if node.kind() == "call_expression" {
        if let Some((method, url_arg)) = match_client_call(node, net_http_names, src) {
            out.push(emit(rel, method, url_arg, fmt_names, src));
        }
    }
    for child in valid_named_children(node) {
        walk(child, rel, src, net_http_names, fmt_names, out);
    }
}

/// `<net/http-name>.<Verb>(url, ...)` -> `(UPPERCASE method, url argument node)`.
fn match_client_call<'t>(
    call: Node<'t>,
    net_http_names: &HashSet<String>,
    src: &str,
) -> Option<(&'static str, Node<'t>)> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "selector_expression" {
        return None;
    }
    let operand = func.child_by_field_name("operand")?;
    let field = func.child_by_field_name("field")?;
    if operand.kind() != "identifier" || !net_http_names.contains(node_text(operand, src)) {
        return None;
    }
    let go_verb = node_text(field, src);
    let (_, method) = VERB_METHODS.iter().find(|(v, _)| *v == go_verb)?;
    let url_arg = nth_arg(call, 0)?;
    Some((method, url_arg))
}

fn emit(
    rel: &str,
    method: &str,
    url_arg: Node,
    fmt_names: &HashSet<String>,
    src: &str,
) -> IoConsume {
    let resolved = resolved_url_literal(url_arg, fmt_names, src);
    let key = resolved.as_deref().and_then(|u| consume_key_for(method, u));
    let (raw, method_field) = match &key {
        Some(_) => (None, None),
        None => (
            Some(node_text(url_arg, src).to_string()),
            Some(method.to_string()),
        ),
    };
    IoConsume {
        kind: "http".to_string(),
        key,
        file: rel.to_string(),
        line: crate::util::line_of(url_arg),
        raw,
        method: method_field,
        body: None,
        client: None,
    }
}

/// A string literal verbatim, or a `fmt.Sprintf("template", ...)` call's normalized template — module
/// doc's "URL resolution" section.
fn resolved_url_literal(node: Node, fmt_names: &HashSet<String>, src: &str) -> Option<String> {
    if let Some(s) = string_literal_text(node, src) {
        return Some(s);
    }
    if node.kind() != "call_expression" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    if func.kind() != "selector_expression" {
        return None;
    }
    let operand = func.child_by_field_name("operand")?;
    let field = func.child_by_field_name("field")?;
    if operand.kind() != "identifier"
        || !fmt_names.contains(node_text(operand, src))
        || node_text(field, src) != "Sprintf"
    {
        return None;
    }
    let template_node = nth_arg(node, 0)?;
    let template = string_literal_text(template_node, src)?;
    Some(normalize_format_verbs(&template))
}

/// Collapses every `%`-verb cluster (`%s`, `%d`, `%05.2f`, `%[1]s`, ...) to `{}`; a literal escaped
/// `%%` is left untouched.
fn normalize_format_verbs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            out.push_str("%%");
            continue;
        }
        for nc in chars.by_ref() {
            if nc.is_ascii_alphabetic() {
                break;
            }
        }
        out.push_str("{}");
    }
    out
}

/// Mirrors `zzop_parser_rust`/`zzop_parser_python_3::adapters::http_clients::consume_key_for` exactly.
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
