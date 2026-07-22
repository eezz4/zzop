//! `reqwest` literal HTTP egress CONSUMES — Rust-side counterpart of `adapters::axum`'s route PROVIDES. Import-gated on `reqwest`; a file that never imports it yields nothing.
//!
//! - **Call shapes**: `reqwest::get(...)`/`reqwest::blocking::get(...)`; and `<recv>.get/.post/.put/
//!   .delete/.patch(...)` gated on a BOUND receiver, mirroring `zzop_parser_python_3`'s import-bound
//!   receiver NAME. Qualifies only as: (1) the free-fn call above; (2) a chain whose leftmost callee
//!   path's first segment is `reqwest` (literal, or a local alias from a `reqwest::...` import), e.g.
//!   `reqwest::Client::new().get(...)`, `reqwest::Client::builder()....build()....get(...)`; or (3) an
//!   identifier/named field bound to `reqwest` by a file-wide first pass: `let <name> = <expr>;` with a
//!   chain-2 RHS; fn/method params and named struct fields typed `reqwest::Client`/`&reqwest::Client`/
//!   `reqwest::blocking::Client`, or a bare `Client` import alias. Shadowing approximation: a name is
//!   bound if ANY visible binding binds it to `reqwest` — deliberately FLAT vs Python's last-write-wins (B14①, 2026-07-22): the collision surface here is narrower (import-gated on `reqwest` + a bound name); upgrade only if a live FP pulls it.
//!
//!   Anything else (`.get`/`.post`/... on an untracked identifier/field, e.g. `cache.get(...)` on a
//!   `HashMap`, `headers.get(...)` on a header map) is NOT a consume: skipped, no unresolved entry
//!   either — replaces an earlier "any receiver in a `reqwest`-importing file" net, found (opus review
//!   F1) to false-key such calls as `reqwest` egress, feeding a false `unprovided-consume` finding. A
//!   `syn::visit::Visit` walk finds every call position; the bound-name pass is a second such walk.
//! - **URL resolution/keying**: a string literal, or `format!(...)` whose FIRST argument has every
//!   `{...}` placeholder collapsed to `{}` (else unresolved). `/`-headed -> keyed via
//!   `http_consume_interface_key`; `http(s)://` -> `"METHOD <url>"`; else `IoConsume{key: None, raw:
//!   Some(<source>), method: Some(<VERB>), ...}` — witnessed, never guessed. No arg skips the call.

use std::collections::HashSet;

use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, ExprCall, ExprMethodCall, Field, FnArg, Lit, Local, Macro, Member, Pat, Type};
use zzop_core::{http_consume_interface_key, ImportMap, IoConsume};

pub(crate) const VERB_METHODS: &[&str] = &["get", "post", "put", "delete", "patch"];

/// Extract this file's `reqwest` HTTP egress consumes — see module doc. Empty on parse failure or import.
pub fn extract_rust_http_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    let Some(file) = crate::parse_file(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    let names = reqwest_local_names(&imports);
    if names.is_empty() {
        return Vec::new();
    }
    let mut bc = BindingCollector {
        names: &names,
        vars: HashSet::new(),
        fields: HashSet::new(),
    };
    bc.visit_file(&file);
    let mut collector = CallCollector {
        rel,
        names: &names,
        vars: &bc.vars,
        fields: &bc.fields,
        out: Vec::new(),
    };
    collector.visit_file(&file);
    collector.out
}

/// Local names bound to a `reqwest`/`reqwest::...` import; empty also doubles as the file-level gate.
fn reqwest_local_names(imports: &ImportMap) -> HashSet<String> {
    imports
        .iter()
        .filter(|(_, b)| b.specifier == "reqwest" || b.specifier.starts_with("reqwest::"))
        .map(|(local, _)| local.clone())
        .collect()
}

/// File-wide first pass (rule 3): collects bound identifier/field names, run once before the call walk.
struct BindingCollector<'a> {
    names: &'a HashSet<String>,
    vars: HashSet<String>,
    fields: HashSet<String>,
}

impl<'a, 'ast> Visit<'ast> for BindingCollector<'a> {
    fn visit_local(&mut self, node: &'ast Local) {
        if let Some(init) = &node.init {
            if chain_is_reqwest(&init.expr, self.names) {
                self.vars.extend(ident_of_pat(&node.pat));
            }
        }
        visit::visit_local(self, node);
    }

    fn visit_fn_arg(&mut self, node: &'ast FnArg) {
        if let FnArg::Typed(pt) = node {
            if type_is_client(&pt.ty, self.names) {
                self.vars.extend(ident_of_pat(&pt.pat));
            }
        }
        visit::visit_fn_arg(self, node);
    }

    fn visit_field(&mut self, node: &'ast Field) {
        if type_is_client(&node.ty, self.names) {
            self.fields
                .extend(node.ident.as_ref().map(|i| i.to_string()));
        }
        visit::visit_field(self, node);
    }
}

/// The bound identifier a `let`/parameter pattern introduces; `None` for a destructuring pattern.
fn ident_of_pat(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(pi) => Some(pi.ident.to_string()),
        Pat::Type(pt) => ident_of_pat(&pt.pat),
        _ => None,
    }
}
/// Rule 3(b)/3(c): is `ty` a `reqwest`/`names`-alias `Client` type, or one behind `&`?
fn type_is_client(ty: &Type, names: &HashSet<String>) -> bool {
    match ty {
        Type::Reference(r) => type_is_client(&r.elem, names),
        Type::Path(tp) => {
            let s = &tp.path.segments;
            match s.len() {
                1 => names.contains(&s[0].ident.to_string()),
                n if n > 1 => s[0].ident == "reqwest" && s[n - 1].ident == "Client",
                _ => false,
            }
        }
        _ => false,
    }
}
/// Rule 2: is `expr`'s call chain rooted at a `reqwest`-first-segment leftmost callee path?
fn chain_is_reqwest(expr: &Expr, names: &HashSet<String>) -> bool {
    match expr {
        Expr::Call(c) => match &*c.func {
            Expr::Path(p) => p
                .path
                .segments
                .first()
                .is_some_and(|s| s.ident == "reqwest" || names.contains(&s.ident.to_string())),
            _ => false,
        },
        Expr::MethodCall(mc) => chain_is_reqwest(&mc.receiver, names),
        _ => false,
    }
}

struct CallCollector<'a> {
    rel: &'a str,
    names: &'a HashSet<String>,
    vars: &'a HashSet<String>,
    fields: &'a HashSet<String>,
    out: Vec<IoConsume>,
}

impl<'a, 'ast> Visit<'ast> for CallCollector<'a> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Some((method, url_arg)) = match_free_fn_call(node) {
            self.emit(method, url_arg);
        }
        visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let method_name = node.method.to_string();
        if VERB_METHODS.contains(&method_name.as_str()) && self.receiver_qualifies(&node.receiver) {
            if let Some(url_arg) = node.args.first() {
                self.emit(method_name.to_ascii_uppercase(), url_arg);
            }
        }
        visit::visit_expr_method_call(self, node);
    }
}

impl<'a> CallCollector<'a> {
    /// Rules 2/3: a chain rooted at `reqwest` (rule 2), a bound identifier, or a bound named field.
    fn receiver_qualifies(&self, receiver: &Expr) -> bool {
        if chain_is_reqwest(receiver, self.names) {
            return true;
        }
        if let Expr::Path(p) = receiver {
            let ident = p.path.get_ident();
            return ident.is_some_and(|i| self.vars.contains(&i.to_string()));
        }
        if let Expr::Field(f) = receiver {
            if let Member::Named(i) = &f.member {
                return self.fields.contains(&i.to_string());
            }
        }
        false
    }

    /// A keyed consume when the URL resolves and keys, else unresolved with the raw source text.
    fn emit(&mut self, method: String, url_arg: &Expr) {
        let key = resolved_url_literal(url_arg).and_then(|r| consume_key_for(&method, &r));
        let text = url_arg.span().source_text().unwrap_or_default();
        let (raw, method) = match &key {
            Some(_) => (None, None),
            None => (Some(text), Some(method)),
        };
        self.out.push(IoConsume {
            kind: "http".to_string(),
            key,
            file: self.rel.to_string(),
            line: crate::line_of(url_arg),
            raw,
            method,
            retry_configured: None,
            body: None,
            client: None,
        });
    }
}

/// Rule 1: `reqwest::get(...)`/... (two segments) or `reqwest::blocking::get(...)`/... (three).
fn match_free_fn_call(call: &ExprCall) -> Option<(String, &Expr)> {
    let Expr::Path(p) = &*call.func else {
        return None;
    };
    let segs = &p.path.segments;
    let n = segs.len();
    let qualifies = (n == 2 || n == 3 && segs[1].ident == "blocking") && segs[0].ident == "reqwest";
    if !qualifies {
        return None;
    }
    let verb = segs[n - 1].ident.to_string();
    if !VERB_METHODS.contains(&verb.as_str()) {
        return None;
    }
    Some((verb.to_ascii_uppercase(), call.args.first()?))
}

/// Resolves a URL argument to a literal string, if statically knowable.
fn resolved_url_literal(expr: &Expr) -> Option<String> {
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
