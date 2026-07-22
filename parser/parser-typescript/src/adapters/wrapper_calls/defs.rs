//! Def recognizer for `wrapper_calls` (`WrapperDefFragment`) — see the parent module doc for the
//! recognizer spec.

use std::sync::OnceLock;

use regex::Regex;
use swc_core::common::{SourceMap, SourceMapper, Span, Spanned};
use swc_core::ecma::ast::{
    BlockStmtOrExpr, Decl, Expr, FnDecl, Module, ModuleDecl, ModuleItem, Pat, Stmt, TsEntityName,
    TsType, TsTypeAnn, VarDecl,
};
use zzop_core::WrapperDefFragment;

/// Every top-level function-like binding — declarations and `const` arrow/function expressions —
/// regardless of export status, keyed to its body text. Feeds `reaches_sink`'s one-hop check: the
/// sink-holding helper is typically NOT exported, so both must be collected the same way.
pub(super) fn collect_top_level_functions(
    module: &Module,
    cm: &SourceMap,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for item in &module.body {
        match item {
            ModuleItem::Stmt(Stmt::Decl(Decl::Fn(f))) => push_fn_decl(f, cm, &mut out),
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(v))) => collect_var_fns(v, cm, &mut out),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) => match &export.decl {
                Decl::Fn(f) => push_fn_decl(f, cm, &mut out),
                Decl::Var(v) => collect_var_fns(v, cm, &mut out),
                _ => {}
            },
            _ => {}
        }
    }
    out
}

fn push_fn_decl(f: &FnDecl, cm: &SourceMap, out: &mut Vec<(String, String)>) {
    if let Some(body) = &f.function.body {
        out.push((
            f.ident.sym.to_string(),
            cm.span_to_snippet(body.span).unwrap_or_default(),
        ));
    }
}

fn collect_var_fns(v: &VarDecl, cm: &SourceMap, out: &mut Vec<(String, String)>) {
    for d in &v.decls {
        let Pat::Ident(bi) = &d.name else { continue };
        let Some(init) = d.init.as_deref() else {
            continue;
        };
        match init {
            Expr::Arrow(a) => {
                let span = match &*a.body {
                    BlockStmtOrExpr::BlockStmt(b) => b.span,
                    BlockStmtOrExpr::Expr(e) => e.span(),
                };
                out.push((
                    bi.id.sym.to_string(),
                    cm.span_to_snippet(span).unwrap_or_default(),
                ));
            }
            Expr::Fn(f) => {
                if let Some(body) = &f.function.body {
                    out.push((
                        bi.id.sym.to_string(),
                        cm.span_to_snippet(body.span).unwrap_or_default(),
                    ));
                }
            }
            _ => {}
        }
    }
}

/// Classifies one candidate function (name + its parameter patterns + its body span, if it has one)
/// as a `WrapperDefFragment`, or `None` when any gate in the module doc's recognizer spec fails.
pub(super) fn classify_def(
    name: &str,
    params: &[&Pat],
    body_span: Option<Span>,
    cm: &SourceMap,
    local_fns: &[(String, String)],
) -> Option<WrapperDefFragment> {
    let body_text = cm.span_to_snippet(body_span?).unwrap_or_default();

    let mut path_param = None;
    let mut method_param = None;
    // One entry per param, index-aligned — feeds the fetch-first-arg path fallback below. A
    // destructuring/rest param (no simple name) is a `None` slot so positions stay correct.
    let mut param_names: Vec<Option<String>> = Vec::with_capacity(params.len());
    for (i, pat) in params.iter().enumerate() {
        let Some((pname, ptype)) = param_info(pat) else {
            param_names.push(None);
            continue;
        };
        if path_param.is_none() && is_path_like(&pname) {
            path_param = Some(i as u32);
        }
        if method_param.is_none() && is_method_param(&pname, ptype.as_deref()) {
            method_param = Some(i as u32);
        }
        param_names.push(Some(pname));
    }

    let sink_body = reaches_sink(name, &body_text, local_fns)?;

    // External-host veto: a wrapper whose sink bakes in an absolute URL would mint internal-looking
    // consume keys for calls that actually leave the analyzed system. External egress is
    // `egress.rs`'s channel, so an absolute-URL sink disqualifies the wrapper (honest under-report
    // over mis-keying).
    if sink_body.contains("http://") || sink_body.contains("https://") {
        return None;
    }

    let is_fetch = mentions_fetch_call(&sink_body);

    // Path-param fallback for a builtin-`fetch` sink whose path param is NOT named path-like (e.g.
    // `export function get(p) { return fetch(p) }`): the wrapper param passed VERBATIM as fetch's
    // first argument IS the path. Only a bare-identifier first arg qualifies (a template/expression
    // first arg leaves this to the name-based signal above — never guessed).
    if path_param.is_none() && is_fetch {
        path_param = fetch_first_arg_param(&sink_body, &param_names);
    }
    let path_param = path_param?;

    let fixed_method = if method_param.is_none() {
        Some(fixed_verb_for_sink(&sink_body, is_fetch)?)
    } else {
        None
    };

    Some(WrapperDefFragment {
        name: name.to_string(),
        method_param,
        path_param,
        fixed_method,
    })
}

/// A parameter's bound name and (when typed with a single-identifier reference, e.g. `: Method`) its
/// type name. Destructuring/rest patterns are never wrapper-signature params.
fn param_info(pat: &Pat) -> Option<(String, Option<String>)> {
    match pat {
        Pat::Ident(bi) => Some((bi.id.sym.to_string(), type_ref_name(bi.type_ann.as_deref()))),
        Pat::Assign(a) => param_info(&a.left),
        _ => None,
    }
}

/// `: Method`-style single-identifier type annotation's name — same pattern `router_mounts.rs`'s own
/// `type_ref_name` uses.
fn type_ref_name(ann: Option<&TsTypeAnn>) -> Option<String> {
    let ann = ann?;
    if let TsType::TsTypeRef(tr) = &*ann.type_ann {
        if let TsEntityName::Ident(id) = &tr.type_name {
            return Some(id.sym.to_string());
        }
    }
    None
}

/// Path-like: case-insensitively `endpoint`/`path`/`url` or a suffix of one — see module doc.
fn is_path_like(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with("endpoint") || lower.ends_with("path") || lower.ends_with("url")
}

fn is_method_param(name: &str, type_name: Option<&str>) -> bool {
    name.eq_ignore_ascii_case("method") || type_name == Some("Method")
}

/// Does `body_text` reach an HTTP sink, directly or one hop through a local helper? Returns the body
/// text that CONTAINS the sink call — what `fixed_verb_for_sink` scans when there's no `method` param.
fn reaches_sink(name: &str, body_text: &str, local_fns: &[(String, String)]) -> Option<String> {
    if has_direct_sink(body_text) {
        return Some(body_text.to_string());
    }
    for (fn_name, fn_body) in local_fns {
        if fn_name == name {
            continue; // never itself
        }
        if calls_identifier(body_text, fn_name) && has_direct_sink(fn_body) {
            return Some(fn_body.clone());
        }
    }
    None
}

fn has_direct_sink(text: &str) -> bool {
    mentions_fetch_call(text)
        || text.contains("axios.")
        || text.contains("axios(")
        || text.contains("ky.")
}

/// A builtin `fetch(...)` call in `text` — `fetch` at a LEFT WORD BOUNDARY (start, or preceded by a
/// non-identifier char, including a `.` member access like `window.fetch`), then `(`. The boundary is
/// load-bearing: a bare `contains("fetch(")` also matches `refetch(` / `prefetch(` (React Query) and
/// any `*fetch` identifier, which — combined with the fetch GET-default and positional-path fallback —
/// fabricated a keyed http consume for a non-HTTP call (opus review, BLOCKING).
fn mentions_fetch_call(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?:^|[^\w$])fetch\s*\(").unwrap());
    re.is_match(text)
}

/// Whether a `fetch(...)` call passes an OPAQUE second argument — an identifier or spread rather than
/// an inline `{...}` object literal (`fetch(url, opts)` / `fetch(url, ...cfg)`). The method may live
/// inside that object, so the bare-`fetch` GET default must NOT apply (never guess a caller-controlled
/// verb — opus review, BLOCKING). An inline `fetch(url, { ... })` is transparent (its verb, if any, is
/// caught by the `method:` literal scan) and does NOT trip this.
fn fetch_has_opaque_options(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?:^|[^\w$])fetch\s*\([^,()]*,\s*(?:[A-Za-z_$]|\.\.\.)").unwrap()
    });
    re.is_match(text)
}

/// Lexical "does this body call `name(...)`" check — precision comes from the param-signature gate
/// above, not a structural call-graph walk here.
fn calls_identifier(text: &str, name: &str) -> bool {
    text.contains(&format!("{name}("))
}

/// The wrapper param index passed VERBATIM as fetch's first argument — the path param when the
/// signature carries no path-like name. Matches ONLY a bare-identifier first arg (`fetch(p)` /
/// `fetch(p, {...})`), the `[,)]` anchor rejecting any composite first arg (`fetch(base + p)`), then
/// resolves that identifier to its param slot. A first-arg identifier that is not a param (e.g. a
/// local `const url`, or a one-hop helper's own param) resolves to nothing — an honest miss.
fn fetch_first_arg_param(sink_body: &str, param_names: &[Option<String>]) -> Option<u32> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE
        .get_or_init(|| Regex::new(r"(?:^|[^\w$])fetch\s*\(\s*([A-Za-z_$][\w$]*)\s*[,)]").unwrap());
    let ident = re.captures(sink_body)?.get(1)?.as_str();
    param_names
        .iter()
        .position(|p| p.as_deref() == Some(ident))
        .map(|i| i as u32)
}

/// The hardcoded verb for a wrapper with no `method` param. A single distinct `method: 'VERB'`
/// literal wins (fetch or axios/ky alike). Zero literals defaults to GET ONLY for a builtin-`fetch`
/// sink with NO `method` key at all — bare `fetch(url)` is a GET; a present-but-dynamic `method`
/// (`fetch(url, { method: verb })`) is never guessed. More than one distinct literal is ambiguous.
fn fixed_verb_for_sink(sink_body: &str, is_fetch: bool) -> Option<String> {
    let verbs = distinct_method_verbs(sink_body);
    if verbs.len() == 1 {
        return verbs.into_iter().next();
    }
    if !verbs.is_empty() {
        return None; // more than one distinct literal verb — ambiguous, disqualify
    }
    if is_fetch && !mentions_method_key(sink_body) && !fetch_has_opaque_options(sink_body) {
        return Some("GET".to_string());
    }
    None
}

/// Distinct UPPERCASED verbs from `method: 'VERB'` literals in `text`, in first-seen order.
fn distinct_method_verbs(text: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"method\s*:\s*['"]([A-Za-z]+)['"]"#).unwrap());
    let mut verbs: Vec<String> = Vec::new();
    for cap in re.captures_iter(text) {
        let verb = cap[1].to_uppercase();
        if !verbs.contains(&verb) {
            verbs.push(verb);
        }
    }
    verbs
}

/// Whether `text` mentions a `method` options key at all — a dynamic (non-literal) `method` present
/// in a fetch options object blocks the bare-`fetch` GET default (never guess a computed verb).
fn mentions_method_key(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\bmethod\b").unwrap());
    re.is_match(text)
}
