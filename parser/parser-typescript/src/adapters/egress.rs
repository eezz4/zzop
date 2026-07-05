//! FE HTTP egress IO extractor — projects the routes a TS/JS tree CONSUMES, so the core cross-layer
//! linker can join each call to its backend handler. Absolute-URL calls are also projected, with a
//! host-carrying key, so they surface as third-party egress instead of being dropped.
//!
//! The crux is constant indirection: a frontend rarely writes `axios.get("/x")`; it writes
//! `axios.get(ControlKey.AUTHEN.getUserInfo)`. We build a project-wide constant map from every
//! top-level object literal first, then resolve each call's URL against it, normalized via
//! `core::http_interface_key` so the key matches whatever the BE adapter emits.
//!
//! Recognized call shapes: `axios.get/post/put/delete/patch(url)`, `ky.get/post/...(url)`,
//! `fetch(url, { method })`, `$fetch(url, { method })`, `axios(url)`, and the oazapfts-generated-SDK
//! family (`oazapfts.fetchJson/fetchText/fetchBlob(url, opts)`, receiver matched tightly, method read
//! from `opts.method` or from an `oazapfts.json/form(...)` wrapper). oazapfts codegen also appends a
//! query-string suffix via a trailing template interpolation starting with `QS.`
//! (`` `/activities${QS.query(...)}` ``) — dropped entirely rather than turned into a `{}` placeholder.

use std::collections::HashMap;

use swc_core::common::{SourceMap, SourceMapper, Spanned};
use swc_core::ecma::ast::{
    CallExpr, Callee, Decl, Expr, ExprOrSpread, Lit, MemberProp, ModuleDecl, ModuleItem, ObjectLit,
    Pat, Prop, PropName, PropOrSpread, Stmt,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_interface_key, IoConsume};

/// Extract HTTP egress IoConsume entries across all files (the const map is project-wide).
pub fn extract_http_egress(files: &[(String, String)]) -> Vec<IoConsume> {
    let consts = build_const_map(files);
    let mut out = Vec::new();
    for (rel, text) in files {
        let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
            continue;
        };
        let cm_ref: &SourceMap = &cm;
        let mut c = EgressCollector {
            cm: cm_ref,
            file: rel,
            consts: &consts,
            out: Vec::new(),
        };
        module.visit_with(&mut c);
        out.extend(c.out);
    }
    out
}

struct EgressCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    consts: &'a HashMap<String, String>,
    out: Vec<IoConsume>,
}

impl Visit for EgressCollector<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Some(hc) = match_http_call(call) {
            let url = resolve_url(hc.arg, self.consts, self.cm);
            let internal = url.as_deref().is_some_and(|u| u.starts_with('/'));
            let external = url.as_deref().is_some_and(is_external);
            // Internal calls join to a same-repo BE route; external calls get a host-carrying key
            // ("METHOD https://host/path", left verbatim rather than run through `http_interface_key`,
            // which would mangle the origin) so `link_cross_layer_io`'s `"://"` gate routes them external.
            let key = if internal {
                Some(http_interface_key(&hc.method, url.as_ref().unwrap()))
            } else if external {
                Some(format!(
                    "{} {}",
                    hc.method.to_uppercase(),
                    url.as_ref().unwrap()
                ))
            } else {
                None
            };
            // Unresolved: the URL couldn't be resolved against THIS call's own `consts` map. `method`
            // travels alongside `raw` so a caller with a wider map can re-resolve it via [`resolve_raw_path`].
            let unresolved = !internal && !external;
            self.out.push(IoConsume {
                kind: "http".into(),
                key,
                file: self.file.into(),
                line: crate::line_of(self.cm, call.span.lo),
                raw: if unresolved {
                    Some(expr_text(hc.arg, self.cm))
                } else {
                    None
                },
                method: if unresolved {
                    Some(hc.method.to_uppercase())
                } else {
                    None
                },
            });
        }
        call.visit_children_with(self); // recurse into nested calls
    }
}

fn is_external(u: &str) -> bool {
    let l = u.to_ascii_lowercase();
    l.starts_with("http://") || l.starts_with("https://")
}

/// Public form of the internal/external classification, exported so `zzop-engine`'s late cross-file
/// resolution can apply the same three-way gating (leading `/` = internal, `http(s)://` = external
/// verbatim key, else unresolved) instead of force-keying every resolved constant as internal.
pub fn is_external_url(u: &str) -> bool {
    is_external(u)
}

struct HttpCall<'a> {
    method: String,
    arg: &'a Expr,
}

fn match_http_call(call: &CallExpr) -> Option<HttpCall<'_>> {
    let arg = &*call.args.first()?.expr;
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    match &**callee {
        Expr::Member(m) => {
            let (Expr::Ident(obj), MemberProp::Ident(name)) = (&*m.obj, &m.prop) else {
                return None;
            };
            let obj = obj.sym.to_string();
            let name = name.sym.to_string();
            if obj == "oazapfts" && is_oazapfts_fetch(&name) {
                let method = method_from_options(call.args.get(1)).unwrap_or_else(|| "GET".into());
                Some(HttpCall { method, arg })
            } else if (obj == "axios" || obj == "ky") && is_http_method(&name) {
                Some(HttpCall { method: name, arg })
            } else {
                None
            }
        }
        Expr::Ident(id) => {
            let n = id.sym.to_string();
            if n == "fetch" || n == "$fetch" {
                let method = method_from_options(call.args.get(1)).unwrap_or_else(|| "GET".into());
                Some(HttpCall { method, arg })
            } else if n == "axios" {
                Some(HttpCall {
                    method: "GET".into(),
                    arg,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_http_method(m: &str) -> bool {
    matches!(m, "get" | "post" | "put" | "delete" | "patch")
}

/// oazapfts codegen's fetch wrappers — `oazapfts.fetchJson/fetchText/fetchBlob(url, opts)`.
fn is_oazapfts_fetch(m: &str) -> bool {
    matches!(m, "fetchJson" | "fetchText" | "fetchBlob")
}

/// Read `{ method: "post" }` from a fetch/axios options object, or from the object literal wrapped in
/// any `oazapfts.<helper>({ ... })` call (`json`/`form`/`multipart`/...; receiver matched, not an
/// allowlist). Only a literal `method: "..."` key is read; `...opts` spreads are silently skipped.
fn method_from_options(opts: Option<&ExprOrSpread>) -> Option<String> {
    let expr = &*opts?.expr;
    let obj = match expr {
        Expr::Object(o) => o,
        Expr::Call(c) => {
            let Callee::Expr(callee) = &c.callee else {
                return None;
            };
            let Expr::Member(m) = &**callee else {
                return None;
            };
            let (Expr::Ident(obj_id), MemberProp::Ident(_)) = (&*m.obj, &m.prop) else {
                return None;
            };
            // Any `oazapfts.<helper>({ ... })` wrapper counts — receiver matched, not a helper allowlist.
            if obj_id.sym != "oazapfts" {
                return None;
            }
            let Expr::Object(o) = &*c.args.first()?.expr else {
                return None;
            };
            o
        }
        _ => return None,
    };
    for prop in &obj.props {
        if let PropOrSpread::Prop(p) = prop {
            if let Prop::KeyValue(kv) = &**p {
                if let (PropName::Ident(name), Expr::Lit(Lit::Str(s))) = (&kv.key, &*kv.value) {
                    if name.sym == "method" {
                        return Some(s.value.as_str().unwrap_or_default().to_uppercase());
                    }
                }
            }
        }
    }
    None
}

/// Resolve a URL argument to a path string, or None if dynamic/unresolvable.
fn resolve_url(arg: &Expr, consts: &HashMap<String, String>, cm: &SourceMap) -> Option<String> {
    match arg {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        Expr::Tpl(t) => {
            // `/api/users/${id}` -> "/api/users/{}". Exception: a *trailing* interpolation starting with
            // `QS.` is oazapfts's query-string suffix (`` `/activities${QS.query(...)}` ``) — dropped
            // entirely rather than turned into `{}`; a non-trailing `QS.`-interpolation keeps `{}`.
            let last_idx = t.exprs.len().checked_sub(1);
            let mut s = String::new();
            for (i, q) in t.quasis.iter().enumerate() {
                s.push_str(
                    q.cooked
                        .as_ref()
                        .and_then(|a| a.as_str())
                        .unwrap_or_default(),
                );
                if i < t.exprs.len() {
                    let is_trailing = last_idx == Some(i)
                        && t.quasis
                            .get(i + 1)
                            .and_then(|q| q.cooked.as_ref())
                            .map(|a| a.as_str().unwrap_or_default().is_empty())
                            .unwrap_or(false);
                    let is_qs_suffix = is_trailing && expr_text(&t.exprs[i], cm).starts_with("QS.");
                    if !is_qs_suffix {
                        s.push_str("{}");
                    }
                }
            }
            Some(s)
        }
        Expr::Ident(_) | Expr::Member(_) => consts.get(&expr_text(arg, cm)).cloned(),
        _ => None,
    }
}

/// Reconstruct a dotted access expression's text ("ControlKey.AUTHEN.getUserInfo"); fall back to the source slice.
fn expr_text(node: &Expr, cm: &SourceMap) -> String {
    match node {
        Expr::Ident(id) => id.sym.to_string(),
        Expr::Member(m) => {
            if let MemberProp::Ident(name) = &m.prop {
                format!("{}.{}", expr_text(&m.obj, cm), name.sym)
            } else {
                cm.span_to_snippet(m.span).unwrap_or_default()
            }
        }
        _ => cm.span_to_snippet(node.span()).unwrap_or_default(),
    }
}

/// One file's constant-map fragment: dotted constant access -> string value, from every top-level
/// `const <Name> = { ... }` in this file's text alone. `build_const_map` folds this over every file; a
/// caller with only one file in hand can merge fragments later and re-resolve via [`resolve_raw_path`].
pub fn const_map_fragment(rel: &str, text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(module) = crate::parse_module(rel, text) else {
        return map;
    };
    for item in &module.body {
        let decl = match item {
            ModuleItem::Stmt(Stmt::Decl(d)) => Some(d),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(e)) => Some(&e.decl),
            _ => None,
        };
        if let Some(Decl::Var(v)) = decl {
            for d in &v.decls {
                if let (Pat::Ident(bi), Some(init)) = (&d.name, &d.init) {
                    if let Expr::Object(obj) = unwrap_expr(init) {
                        flatten(bi.id.sym.as_ref(), obj, &mut map);
                    }
                }
            }
        }
    }
    map
}

/// Project-wide map of dotted constant access -> string value — the fold over [`const_map_fragment`]. A
/// key duplicated across two files resolves to whichever file's fragment is folded in last.
fn build_const_map(files: &[(String, String)]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (rel, text) in files {
        map.extend(const_map_fragment(rel, text));
    }
    map
}

/// Late re-resolution: resolves a consume's `raw` text against a (possibly wider) constant map if `raw`
/// is a plain dotted identifier chain present in `consts`; anything else (a call, a template, a bracket
/// access, a bare identifier with no dot) returns `None`.
pub fn resolve_raw_path(raw: &str, consts: &HashMap<String, String>) -> Option<String> {
    let trimmed = raw.trim();
    if !is_dotted_identifier_chain(trimmed) {
        return None;
    }
    consts.get(trimmed).cloned()
}

/// True for a plain dotted identifier chain with no calls/brackets/templates (`Foo.bar.baz`), false for
/// anything else including a single bare identifier with no dot (`+` requires at least one `.segment`).
fn is_dotted_identifier_chain(s: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^[A-Za-z_$][\w$]*(\.[A-Za-z_$][\w$]*)+$").unwrap())
        .is_match(s)
}

fn flatten(prefix: &str, obj: &ObjectLit, map: &mut HashMap<String, String>) {
    for prop in &obj.props {
        let PropOrSpread::Prop(p) = prop else {
            continue;
        };
        let Prop::KeyValue(kv) = &**p else { continue };
        let name = match &kv.key {
            PropName::Ident(i) => i.sym.to_string(),
            PropName::Str(s) => s.value.as_str().unwrap_or_default().to_string(),
            _ => continue,
        };
        let key = format!("{prefix}.{name}");
        match unwrap_expr(&kv.value) {
            Expr::Lit(Lit::Str(s)) => {
                map.insert(key, s.value.as_str().unwrap_or_default().to_string());
            }
            Expr::Object(o) => flatten(&key, o, map),
            _ => {}
        }
    }
}

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

#[cfg(test)]
mod tests {
    //! Coverage for `extract_http_egress`: HTTP call-site detection, URL resolution
    //! (literal/template/const-indirection), and internal-vs-external classification.
    use super::*;

    fn files(xs: &[(&str, &str)]) -> Vec<(String, String)> {
        xs.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }
    fn keys(out: &[IoConsume]) -> Vec<Option<String>> {
        out.iter().map(|c| c.key.clone()).collect()
    }

    #[test]
    fn captures_internal_axios_string_literal() {
        let out = extract_http_egress(&files(&[("a.tsx", r#"axios.get("/authen/getUserInfo")"#)]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "http");
        assert_eq!(out[0].key.as_deref(), Some("GET /authen/getUserInfo"));
        assert_eq!(out[0].file, "a.tsx");
        assert_eq!(out[0].line, 1);
        assert!(out[0].raw.is_none());
    }

    #[test]
    fn resolves_cross_file_controlkey_indirection() {
        let out = extract_http_egress(&files(&[
            ("protocol/ControlKey.ts", r#"export const ControlKey = { AUTHEN: { getUserInfo: "/authen/getUserInfo", getSignout: "/authen/getSignout" } };"#),
            ("Ctx.tsx", "axios.get(ControlKey.AUTHEN.getUserInfo); axios.get(ControlKey.AUTHEN.getSignout);"),
        ]));
        assert_eq!(
            keys(&out),
            vec![
                Some("GET /authen/getUserInfo".to_string()),
                Some("GET /authen/getSignout".to_string())
            ]
        );
    }

    #[test]
    fn resolves_as_const() {
        let out = extract_http_egress(&files(&[
            (
                "protocol/ControlKey.ts",
                r#"export const ControlKey = { AUTHEN: { getUserInfo: "/authen/getUserInfo" } } as const;"#,
            ),
            ("Ctx.tsx", "axios.get(ControlKey.AUTHEN.getUserInfo)"),
        ]));
        assert_eq!(out[0].key.as_deref(), Some("GET /authen/getUserInfo"));
    }

    #[test]
    fn derives_method_from_post_and_fetch_options() {
        let out = extract_http_egress(&files(&[
            ("k.ts", r#"const K = { create: "/items/create" };"#),
            (
                "p.tsx",
                r#"axios.post(K.create); fetch("/items/create", { method: "delete" });"#,
            ),
        ]));
        assert_eq!(
            keys(&out),
            vec![
                Some("POST /items/create".to_string()),
                Some("DELETE /items/create".to_string())
            ]
        );
    }

    #[test]
    fn normalizes_template_literal_params() {
        let out = extract_http_egress(&files(&[("t.tsx", "axios.get(`/api/users/${id}/posts`)")]));
        assert_eq!(out[0].key.as_deref(), Some("GET /api/users/{}/posts"));
    }

    #[test]
    fn absolute_url_becomes_a_host_carrying_key_for_the_external_bucket() {
        let out = extract_http_egress(&files(&[(
            "e.tsx",
            r#"axios.get("https://api.stripe.com/v1/charges")"#,
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].key.as_deref(),
            Some("GET https://api.stripe.com/v1/charges")
        );
        assert!(out[0].raw.is_none());
    }

    #[test]
    fn marks_dynamic_url_as_null_with_raw() {
        let out = extract_http_egress(&files(&[("d.tsx", "axios.get(buildUrl(x))")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("buildUrl(x)"));
        // Carried for late re-resolution even though `buildUrl(x)` is not itself a dotted chain.
        assert_eq!(out[0].method.as_deref(), Some("GET"));
    }

    #[test]
    fn cross_file_constant_indirection_unresolved_consume_carries_its_method() {
        // Only THIS file is visible, so `ControlKey` never resolves here — but `method` must still be set
        // so a caller with a wider constant map can key the consume once it does resolve.
        let out = extract_http_egress(&files(&[(
            "Ctx.tsx",
            "axios.post(ControlKey.AUTHEN.getUserInfo);",
        )]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("ControlKey.AUTHEN.getUserInfo"));
        assert_eq!(out[0].method.as_deref(), Some("POST"));
    }

    #[test]
    fn ignores_non_http_calls() {
        let out = extract_http_egress(&files(&[("x.ts", r#"foo.get("/a"); console.log("/b");"#)]));
        assert!(out.is_empty());
    }

    // --- oazapfts-generated-SDK call family ---

    #[test]
    fn oazapfts_get_with_trailing_qs_suffix_drops_the_interpolation() {
        let out = extract_http_egress(&files(&[(
            "activity.ts",
            r#"oazapfts.ok(oazapfts.fetchJson<{ status: 200 }>(`/activities${QS.query(QS.explode({ albumId }))}`, { ...opts }));"#,
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /activities"));
    }

    #[test]
    fn oazapfts_post_reads_method_from_oazapfts_json_wrapper() {
        let out = extract_http_egress(&files(&[(
            "activity.ts",
            r#"oazapfts.ok(oazapfts.fetchJson<{ status: 201 }>("/activities", oazapfts.json({ ...opts, method: "POST", body: activityCreateDto })));"#,
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("POST /activities"));
    }

    #[test]
    fn oazapfts_post_reads_method_from_oazapfts_multipart_wrapper() {
        let out = extract_http_egress(&files(&[(
            "backup.ts",
            r#"oazapfts.fetchJson("/admin/backups/upload", oazapfts.multipart({ ...opts, method: "POST", body }))"#,
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("POST /admin/backups/upload"));
    }

    #[test]
    fn oazapfts_call_nested_inside_oazapfts_ok_is_still_detected() {
        // `oazapfts.ok(...)` itself is not a recognized callee — passes only via the visitor's existing
        // recursion into nested calls, without any special-casing of the `ok` wrapper.
        let out = extract_http_egress(&files(&[(
            "a.ts",
            r#"return oazapfts.ok(oazapfts.fetchJson("/activities", { ...opts }));"#,
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /activities"));
    }

    #[test]
    fn oazapfts_fetch_blob_with_path_param_template_keeps_placeholder() {
        let out = extract_http_egress(&files(&[(
            "asset.ts",
            r#"oazapfts.ok(oazapfts.fetchBlob(`/assets/${id}/thumbnail`, { ...opts }));"#,
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /assets/{}/thumbnail"));
    }

    #[test]
    fn mid_path_qs_interpolation_is_not_the_trailing_suffix_shape_and_keeps_placeholder() {
        let out = extract_http_egress(&files(&[(
            "weird.ts",
            r#"oazapfts.ok(oazapfts.fetchJson(`/foo/${QS.query(x)}/bar`, { ...opts }));"#,
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /foo/{}/bar"));
    }

    #[test]
    fn non_oazapfts_receiver_with_same_method_name_is_not_recognized() {
        let out = extract_http_egress(&files(&[(
            "other.ts",
            r#"other.fetchJson("/activities", { ...opts });"#,
        )]));
        assert!(out.is_empty());
    }

    // --- const_map_fragment / resolve_raw_path (late cross-file consume resolution substrate) ---

    #[test]
    fn const_map_fragment_flattens_one_files_nested_object_consts() {
        let frag = const_map_fragment(
            "protocol/ControlKey.ts",
            r#"export const ControlKey = { AUTHEN: { getUserInfo: "/authen/getUserInfo" } };"#,
        );
        assert_eq!(
            frag.get("ControlKey.AUTHEN.getUserInfo")
                .map(String::as_str),
            Some("/authen/getUserInfo")
        );
    }

    #[test]
    fn const_map_fragment_is_empty_for_a_file_with_no_top_level_object_const() {
        let frag = const_map_fragment("x.ts", "export const n = 1;\n");
        assert!(frag.is_empty());
    }

    #[test]
    fn resolve_raw_path_hits_a_dotted_chain_present_in_the_map() {
        let mut consts = HashMap::new();
        consts.insert(
            "ControlKey.AUTHEN.getUserInfo".to_string(),
            "/authen/getUserInfo".to_string(),
        );
        assert_eq!(
            resolve_raw_path("ControlKey.AUTHEN.getUserInfo", &consts).as_deref(),
            Some("/authen/getUserInfo")
        );
    }

    #[test]
    fn resolve_raw_path_misses_a_dotted_chain_absent_from_the_map() {
        let consts = HashMap::new();
        assert_eq!(
            resolve_raw_path("ControlKey.AUTHEN.getUserInfo", &consts),
            None
        );
    }

    #[test]
    fn resolve_raw_path_rejects_a_call_expression() {
        let mut consts = HashMap::new();
        consts.insert("buildUrl".to_string(), "/should/not/match".to_string());
        assert_eq!(resolve_raw_path("buildUrl(x)", &consts), None);
    }

    #[test]
    fn resolve_raw_path_rejects_a_template_literal() {
        let consts = HashMap::new();
        assert_eq!(resolve_raw_path("`/api/${id}`", &consts), None);
    }

    #[test]
    fn resolve_raw_path_rejects_a_bare_identifier_with_no_dot() {
        let mut consts = HashMap::new();
        // Even if a bare name happened to be a map key (it never is — `flatten` only inserts dotted
        // keys), an identifier with no `.` must still be rejected: the regex requires one `.segment`.
        consts.insert("ControlKey".to_string(), "/should/not/match".to_string());
        assert_eq!(resolve_raw_path("ControlKey", &consts), None);
    }
}
