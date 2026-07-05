//! FE wrapper-function CONSUME resolution, stage 1 — the provide-side is `router_mounts.rs`/
//! `trpc_router.rs`'s sibling for the CONSUME half of the cross-layer IO join: a frontend codebase
//! often wraps its real HTTP-call sink (`fetch`/`axios`/`ky`) behind a project-local helper instead of
//! calling the sink directly at every use site — e.g. `makeRestApiRequest(context, method, endpoint,
//! data?)` forwarding to a SIBLING `request()` that calls `axios.request(...)`. Without this
//! projection those call sites' HTTP consume would either not exist (the plain-egress extractor in
//! `egress.rs` only recognizes direct sink calls) or anchor back to the wrapper's own definition site,
//! not the call site. This module only projects one file's local facts; the actual re-anchoring
//! (resolving a call's callee to a def fragment, possibly cross-file via `specifier`) is the engine's
//! assemble-time join — see `zpz_core::fragments`'s `WrapperDefFragment`/`WrapperCallFragment` doc.
//!
//! ## Def recognizer (`WrapperDefFragment`)
//! An EXPORTED top-level function/const-arrow qualifies as a wrapper def when ALL of:
//! - a parameter's name, case-insensitively, is or ENDS IN `endpoint`/`path`/`url` (e.g.
//!   `apiEndpoint`) -> its index becomes `path_param`. No type annotation required — same
//!   "name is the signal" tradeoff `router_mounts.rs`/`controller_decorators.rs` document;
//! - a `method` (or `: Method`-typed) parameter -> `method_param`, OR — absent that — the reachable
//!   sink body contains exactly ONE distinct `method: 'VERB'` literal -> `fixed_method` (zero or
//!   ambiguous verbs disqualify the function);
//! - its body reaches a sink call `fetch(`/`axios.`/`axios(`/`ky.`, directly or one hop through a
//!   LOCAL top-level helper (declared or const arrow/fn, exported or not) whose body contains the
//!   sink. Exactly one hop — a helper forwarding to ANOTHER helper is not walked further.
//!
//! The sink check is a lexical substring test (`SourceMap::span_to_snippet`), not structural — cheap,
//! with precision carried by the param-signature gate above.
//!
//! ## Call recognizer (`WrapperCallFragment`)
//! Every call whose callee is a PLAIN identifier (member-expression callees are out of scope) is a
//! candidate when one of its first 6 args is an uppercase HTTP verb literal or a string/template
//! starting with `/` (volume guard, else every `helper(a, b)` call would qualify). The call side does
//! NOT resolve whether `callee` names a known def — defs often live in a different file; the
//! assemble-time join filters candidates down to real invocations.
//!
//! Each of the first 6 args is captured positionally: string literal verbatim, template literal with
//! `${...}` replaced by `{}` (same transform as `egress.rs`'s `resolve_url`), anything else `None` —
//! never guessed. `specifier` comes from the file's import map when `callee` is an imported binding;
//! `None` means local-or-unresolved (assemble only resolves same-file when `specifier` is `None`).
//!
//! Deterministic output: defs and calls in source (AST-walk) order; no matches -> two empty vecs.

use std::sync::OnceLock;

use regex::Regex;
use swc_core::common::{SourceMap, SourceMapper, Span, Spanned};
use swc_core::ecma::ast::{
    BlockStmtOrExpr, CallExpr, Callee, Decl, Expr, FnDecl, Lit, Module, ModuleDecl, ModuleItem,
    Pat, Stmt, Tpl, TsEntityName, TsType, TsTypeAnn, VarDecl,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zpz_core::{ImportMap, WrapperCallFragment, WrapperDefFragment};

/// Extract one file's wrapper-def and wrapper-call fragments — see module doc for the recognizer spec.
pub fn extract_wrapper_fragments(
    rel: &str,
    text: &str,
) -> (Vec<WrapperDefFragment>, Vec<WrapperCallFragment>) {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return (Vec::new(), Vec::new());
    };
    let imports = crate::parse_imports(rel, text);
    let local_fns = collect_top_level_functions(&module, &cm);

    let mut defs = Vec::new();
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) = item else {
            continue;
        };
        match &export.decl {
            Decl::Fn(f) => {
                let pats: Vec<&Pat> = f.function.params.iter().map(|p| &p.pat).collect();
                let body_span = f.function.body.as_ref().map(|b| b.span);
                if let Some(d) = classify_def(&f.ident.sym, &pats, body_span, &cm, &local_fns) {
                    defs.push(d);
                }
            }
            Decl::Var(v) => {
                for d in &v.decls {
                    let Pat::Ident(bi) = &d.name else { continue };
                    let Some(Expr::Arrow(arrow)) = d.init.as_deref() else {
                        continue;
                    };
                    let pats: Vec<&Pat> = arrow.params.iter().collect();
                    let body_span = Some(match &*arrow.body {
                        BlockStmtOrExpr::BlockStmt(b) => b.span,
                        BlockStmtOrExpr::Expr(e) => e.span(),
                    });
                    if let Some(frag) = classify_def(&bi.id.sym, &pats, body_span, &cm, &local_fns)
                    {
                        defs.push(frag);
                    }
                }
            }
            _ => {}
        }
    }

    let mut calls = Vec::new();
    let mut collector = CallCollector {
        cm: &cm,
        imports: &imports,
        out: &mut calls,
    };
    module.visit_with(&mut collector);

    (defs, calls)
}

/// Every top-level function-like binding — declarations and `const` arrow/function expressions —
/// regardless of export status, keyed to its body text. Feeds `reaches_sink`'s one-hop check: the
/// sink-holding helper is typically NOT exported, so both must be collected the same way.
fn collect_top_level_functions(module: &Module, cm: &SourceMap) -> Vec<(String, String)> {
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
fn classify_def(
    name: &str,
    params: &[&Pat],
    body_span: Option<Span>,
    cm: &SourceMap,
    local_fns: &[(String, String)],
) -> Option<WrapperDefFragment> {
    let body_text = cm.span_to_snippet(body_span?).unwrap_or_default();

    let mut path_param = None;
    let mut method_param = None;
    for (i, pat) in params.iter().enumerate() {
        let Some((pname, ptype)) = param_info(pat) else {
            continue;
        };
        if path_param.is_none() && is_path_like(&pname) {
            path_param = Some(i as u32);
        }
        if method_param.is_none() && is_method_param(&pname, ptype.as_deref()) {
            method_param = Some(i as u32);
        }
    }
    let path_param = path_param?;

    let sink_body = reaches_sink(name, &body_text, local_fns)?;

    // External-host veto: a wrapper whose sink bakes in an absolute URL would mint internal-looking
    // consume keys for calls that actually leave the analyzed system. External egress is
    // `egress.rs`'s channel, so an absolute-URL sink disqualifies the wrapper (honest under-report
    // over mis-keying).
    if sink_body.contains("http://") || sink_body.contains("https://") {
        return None;
    }

    let fixed_method = if method_param.is_none() {
        Some(single_fixed_verb(&sink_body)?)
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
/// text that CONTAINS the sink call — what `single_fixed_verb` scans when there's no `method` param.
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
    text.contains("fetch(")
        || text.contains("axios.")
        || text.contains("axios(")
        || text.contains("ky.")
}

/// Lexical "does this body call `name(...)`" check — precision comes from the param-signature gate
/// above, not a structural call-graph walk here.
fn calls_identifier(text: &str, name: &str) -> bool {
    text.contains(&format!("{name}("))
}

/// Scans `text` for `method: 'VERB'` literals; returns the single distinct UPPERCASED verb, or
/// `None` when zero or more than one distinct verb is present (ambiguous — see module doc).
fn single_fixed_verb(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"method\s*:\s*['"]([A-Za-z]+)['"]"#).unwrap());
    let mut verbs: Vec<String> = Vec::new();
    for cap in re.captures_iter(text) {
        let verb = cap[1].to_uppercase();
        if !verbs.contains(&verb) {
            verbs.push(verb);
        }
    }
    match verbs.len() {
        1 => verbs.into_iter().next(),
        _ => None,
    }
}

/// Whole-module walk collecting every candidate wrapper call site — see module doc's call recognizer.
struct CallCollector<'a> {
    cm: &'a SourceMap,
    imports: &'a ImportMap,
    out: &'a mut Vec<WrapperCallFragment>,
}

impl Visit for CallCollector<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Some(frag) = self.classify_call(call) {
            self.out.push(frag);
        }
        call.visit_children_with(self); // recurse — a qualifying call's own args can nest further calls
    }
}

impl CallCollector<'_> {
    fn classify_call(&self, call: &CallExpr) -> Option<WrapperCallFragment> {
        let Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let Expr::Ident(id) = &**callee else {
            return None; // member/other callee shapes are out of scope for this stage
        };
        let callee_name = id.sym.to_string();

        let mut args: Vec<Option<String>> = Vec::new();
        let mut has_verb_or_slash = false;
        for a in call.args.iter().take(6) {
            let captured = if a.spread.is_some() {
                None
            } else {
                capture_arg(&a.expr)
            };
            if let Some(text) = &captured {
                if is_uppercase_verb(text) || text.starts_with('/') {
                    has_verb_or_slash = true;
                }
            }
            args.push(captured);
        }
        if !has_verb_or_slash {
            return None; // volume guard — see module doc
        }

        let specifier = self.imports.get(&callee_name).map(|b| b.specifier.clone());
        Some(WrapperCallFragment {
            callee: callee_name,
            specifier,
            args,
            line: crate::line_of(self.cm, call.span.lo),
        })
    }
}

fn is_uppercase_verb(s: &str) -> bool {
    matches!(s, "GET" | "POST" | "PUT" | "PATCH" | "DELETE")
}

/// A call argument's literal capture — see module doc's positional-capture rules.
fn capture_arg(e: &Expr) -> Option<String> {
    match unwrap_expr(e) {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        Expr::Tpl(t) => Some(tpl_shape(t)),
        _ => None,
    }
}

/// `` `/workflows/${id}/activate` `` -> `"/workflows/{}/activate"` — same transform `egress.rs`'s own
/// `resolve_url` applies (minus its oazapfts `QS.`-suffix special case, not relevant here).
fn tpl_shape(t: &Tpl) -> String {
    let mut s = String::new();
    for (i, q) in t.quasis.iter().enumerate() {
        s.push_str(
            q.cooked
                .as_ref()
                .and_then(|a| a.as_str())
                .unwrap_or_default(),
        );
        if i < t.exprs.len() {
            s.push_str("{}");
        }
    }
    s
}

/// Strip `as`/paren/`satisfies`/non-null wrappers — same set `trpc_router.rs`'s own `unwrap_expr` strips.
fn unwrap_expr(e: &Expr) -> &Expr {
    let mut n = e;
    loop {
        n = match n {
            Expr::TsAs(a) => &a.expr,
            Expr::TsConstAssertion(c) => &c.expr,
            Expr::Paren(p) => &p.expr,
            Expr::TsSatisfies(s) => &s.expr,
            Expr::TsNonNull(nn) => &nn.expr,
            other => return other,
        };
    }
}

#[cfg(test)]
mod tests {
    //! Coverage: one-hop-sink via a sibling helper, fixed-method via param-name suffix, a non-wrapper
    //! fn with no reachable sink, call-site argument capture, import-specifier resolution, the volume
    //! guard, determinism, and the empty-file case.
    use super::*;

    fn def<'a>(defs: &'a [WrapperDefFragment], name: &str) -> &'a WrapperDefFragment {
        defs.iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("no def named {name:?} in {defs:?}"))
    }

    #[test]
    fn one_hop_sink_via_sibling_request_shape() {
        let src = concat!(
            "async function request(config) {\n",
            "  return axios.request(config);\n",
            "}\n",
            "\n",
            "export async function makeRestApiRequest<T>(\n",
            "  context: IRestApiContext,\n",
            "  method: Method,\n",
            "  endpoint: string,\n",
            "  data?: any,\n",
            "): Promise<T> {\n",
            "  const response = await request({ method, baseURL: context.baseUrl, endpoint, data });\n",
            "  return response.data;\n",
            "}\n",
            "\n",
            "export async function getFullApiResponse<T>(\n",
            "  context: IRestApiContext,\n",
            "  method: Method,\n",
            "  endpoint: string,\n",
            "): Promise<T> {\n",
            "  return request({ method, baseURL: context.baseUrl, endpoint });\n",
            "}\n"
        );
        let (defs, _) = extract_wrapper_fragments("apiRequest.ts", src);
        assert_eq!(defs.len(), 2, "{defs:?}");

        let make = def(&defs, "makeRestApiRequest");
        assert_eq!(make.method_param, Some(1));
        assert_eq!(make.path_param, 2);
        assert_eq!(make.fixed_method, None);

        let full = def(&defs, "getFullApiResponse");
        assert_eq!(full.method_param, Some(1));
        assert_eq!(full.path_param, 2);
        assert_eq!(full.fixed_method, None);
    }

    #[test]
    fn fixed_method_wrapper_via_endpoint_suffix_param_name() {
        let src = concat!(
            "export async function streamRequest(ctx, apiEndpoint: string, payload) {\n",
            "  const url = apiEndpoint;\n",
            "  return fetch(url, { method: 'POST', body: JSON.stringify(payload) });\n",
            "}\n"
        );
        let (defs, _) = extract_wrapper_fragments("stream.ts", src);
        let d = def(&defs, "streamRequest");
        assert_eq!(d.method_param, None);
        assert_eq!(d.path_param, 1);
        assert_eq!(d.fixed_method.as_deref(), Some("POST"));
    }

    #[test]
    fn exported_fn_with_url_param_but_no_sink_is_not_a_wrapper() {
        let src = concat!(
            "export function buildUrl(base: string, url: string): string {\n",
            "  return base + url;\n",
            "}\n"
        );
        let (defs, _) = extract_wrapper_fragments("build-url.ts", src);
        assert!(defs.is_empty(), "{defs:?}");
    }

    #[test]
    fn external_host_sink_disqualifies_the_wrapper() {
        // A per-service fetcher with an absolute-URL sink: call sites pass internal-LOOKING paths
        // that actually leave the system — see the external-host veto in `classify_def`.
        let src = concat!(
            "export const fetcher = async (endpoint: string, init?: RequestInit) => {\n",
            "  return fetch(`https://api.example.com/v1${endpoint}`, { method: \"GET\", ...init });\n",
            "};\n"
        );
        let (defs, _) = extract_wrapper_fragments("exampleApiFetcher.ts", src);
        assert!(defs.is_empty(), "{defs:?}");
    }

    #[test]
    fn ambiguous_fixed_method_two_distinct_verbs_is_not_a_wrapper() {
        let src = concat!(
            "export function poll(url: string, mode: string) {\n",
            "  if (mode === 'a') { return fetch(url, { method: 'GET' }); }\n",
            "  return fetch(url, { method: 'POST' });\n",
            "}\n"
        );
        let (defs, _) = extract_wrapper_fragments("poll.ts", src);
        assert!(defs.is_empty(), "{defs:?}");
    }

    #[test]
    fn call_site_positional_capture_literal_and_template_and_specifier() {
        let src = concat!(
            "import { makeRestApiRequest } from './helper';\n",
            "function useThing() {\n",
            "  makeRestApiRequest(context, 'GET', '/workflows/new');\n",
            "  makeRestApiRequest(ctx, 'POST', `/workflows/${id}/activate`, data);\n",
            "}\n"
        );
        let (_, calls) = extract_wrapper_fragments("use-thing.ts", src);
        assert_eq!(calls.len(), 2, "{calls:?}");

        assert_eq!(calls[0].callee, "makeRestApiRequest");
        assert_eq!(calls[0].specifier.as_deref(), Some("./helper"));
        assert_eq!(
            calls[0].args,
            vec![None, Some("GET".into()), Some("/workflows/new".into())]
        );
        assert_eq!(calls[0].line, 3);

        assert_eq!(calls[1].specifier.as_deref(), Some("./helper"));
        assert_eq!(
            calls[1].args,
            vec![
                None,
                Some("POST".into()),
                Some("/workflows/{}/activate".into()),
                None,
            ]
        );
    }

    #[test]
    fn local_callee_has_no_specifier() {
        let src = "function f() {\n  localHelper('GET', '/x');\n}\n";
        let (_, calls) = extract_wrapper_fragments("local.ts", src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].callee, "localHelper");
        assert_eq!(calls[0].specifier, None);
    }

    #[test]
    fn call_with_no_verb_and_no_slash_arg_is_not_captured() {
        let src = concat!(
            "function f(x, y) {\n",
            "  noop(x, y);\n",
            "  helper('abc', 'def');\n",
            "}\n"
        );
        let (_, calls) = extract_wrapper_fragments("skip.ts", src);
        assert!(calls.is_empty(), "{calls:?}");
    }

    #[test]
    fn deterministic_across_repeated_extractions() {
        let src = concat!(
            "async function request(config) {\n",
            "  return axios.request(config);\n",
            "}\n",
            "export async function makeRestApiRequest(context, method: Method, endpoint: string) {\n",
            "  return request({ method, endpoint });\n",
            "}\n",
            "makeRestApiRequest(ctx, 'GET', '/a');\n"
        );
        let a = extract_wrapper_fragments("d.ts", src);
        let b = extract_wrapper_fragments("d.ts", src);
        assert_eq!(a, b);
    }

    #[test]
    fn empty_file_yields_no_fragments() {
        let (defs, calls) = extract_wrapper_fragments("e.ts", "");
        assert!(defs.is_empty());
        assert!(calls.is_empty());
    }
}
