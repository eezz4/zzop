//! URL resolution and consume KEYING for `adapters::http_clients` — the half of that adapter that never
//! looks at the AST's call shape, only at the one argument the call handed over. Split out of the parent
//! module (2026-08-02) when the test-surface gate was added there and the file reached its line cap: the
//! two halves already had no shared state, so the seam was where it was going to be anyway.
//!
//! Nothing here decides WHETHER a call is egress; that is the parent's bound-receiver logic. Everything
//! here decides what a call that already qualified is keyed AS, and it never guesses — an argument that
//! does not resolve to a literal comes back `None`, and the parent records it as an unresolved consume
//! carrying the raw source text.

use syn::{Expr, Lit, Macro};
use zzop_core::http_consume_interface_key;

/// Resolves a URL argument to a literal string, if statically knowable.
pub(super) fn resolved_url_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(el) => match &el.lit {
            Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        Expr::Reference(r) => resolved_url_literal(&r.expr),
        Expr::Macro(em) => format_macro_literal(&em.mac),
        _ => None,
    }
}

/// `format!("template", args...)` -> the template with every `{...}` placeholder collapsed to `{}`.
/// `None` for any other macro or unparseable/non-literal first argument.
fn format_macro_literal(mac: &Macro) -> Option<String> {
    if !mac.path.is_ident("format") {
        return None;
    }
    let exprs = mac
        .parse_body_with(syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated)
        .ok()?;
    let Expr::Lit(el) = exprs.first()? else {
        return None;
    };
    let Lit::Str(s) = &el.lit else { return None };
    Some(normalize_placeholders(&s.value()))
}

/// Collapses every `{...}` placeholder to `{}`, leaving an escaped `{{`/`}}` literal brace untouched.
fn normalize_placeholders(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
                out.push_str("{{");
                continue;
            }
            for nc in chars.by_ref() {
                if nc == '}' {
                    break;
                }
            }
            out.push_str("{}");
        } else if c == '}' && chars.peek() == Some(&'}') {
            chars.next();
            out.push_str("}}");
        } else {
            out.push(c);
        }
    }
    out
}

/// Mirrors `zzop_parser_python_3::adapters::http_clients::consume_key_for` exactly.
pub(super) fn consume_key_for(method: &str, url: &str) -> Option<String> {
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
