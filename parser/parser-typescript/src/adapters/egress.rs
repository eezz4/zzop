//! FE HTTP egress IO extractor — projects the routes a TS/JS tree CONSUMES, so the core cross-layer
//! linker can join each call to its backend handler. Absolute-URL calls are also projected, with a
//! host-carrying key, so they surface as third-party egress instead of being dropped.
//!
//! The crux is constant indirection: a frontend rarely writes `axios.get("/x")`; it writes
//! `axios.get(ControlKey.AUTHEN.getUserInfo)`. We build a project-wide constant map from every
//! top-level object literal first, then resolve each call's URL against it, normalized via
//! `core::http_consume_interface_key` (which also drops any `?...`/`#...` query/fragment suffix —
//! `query-drop-v1`) so the key matches whatever the BE adapter emits.
//!
//! Recognized call shapes: `axios.get/post/put/delete/patch(url)`, `ky.get/post/...(url)`,
//! `fetch(url, { method })`, `$fetch(url, { method })`, `axios(url)`, the oazapfts-generated-SDK
//! family (`oazapfts.fetchJson/fetchText/fetchBlob(url, opts)`, receiver matched tightly, method read
//! from `opts.method` or from an `oazapfts.json/form(...)` wrapper), and a computed member callee on
//! `axios`/`ky` (`axios['post'](url)`, `axios[favorited ? 'delete' : 'post'](url)`) whose bracket
//! expression is a recognized-verb string literal or a ternary with two such literal arms. oazapfts
//! codegen also appends a query-string suffix via a trailing template interpolation starting with
//! `QS.` (`` `/activities${QS.query(...)}` ``) — dropped entirely rather than turned into a `{}`
//! placeholder.
//!
//! `cond-literal-fanout-v1`: a ternary with two string-literal arms — as the whole URL argument, as a
//! template interpolation, or as the computed-member method — enumerates one deterministic key per arm
//! instead of collapsing to `{}`/going unrecognized. Both arms are visible literals in the source, so
//! this is normalization of visible facts, not speculation (the "never guess" convention only forbids
//! inventing values that aren't written down). Template fan-out is capped at 2 conditional-literal
//! interpolations (≤4 variants); a 3rd+ interpolation of that shape falls back to `{}` for ALL of them
//! in that template, keeping output bounded and deterministic.

use std::collections::{HashMap, HashSet};

use swc_core::common::{SourceMap, SourceMapper, Spanned};
use swc_core::ecma::ast::{
    CallExpr, Callee, Decl, Expr, ExprOrSpread, Lit, MemberProp, ModuleDecl, ModuleItem, ObjectLit,
    Pat, Prop, PropName, PropOrSpread, Stmt, Tpl, TsEnumMemberId,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_consume_interface_key, IoConsume};

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
            let url_variants = resolve_url_variants(hc.arg, self.consts, self.cm);
            let line = crate::line_of(self.cm, call.span.lo);
            if url_variants.is_empty() {
                // Unresolved: the URL couldn't be resolved against THIS call's own `consts` map at all
                // (no variants produced). One consume PER METHOD so a caller with a wider constant map
                // can re-resolve each method branch independently via [`resolve_raw_path`]; with a single
                // method this is exactly today's one-consume behavior.
                let raw = expr_text(hc.arg, self.cm);
                for method in &hc.methods {
                    self.out.push(IoConsume {
                        kind: "http".into(),
                        key: None,
                        file: self.file.into(),
                        line,
                        raw: Some(raw.clone()),
                        method: Some(method.to_uppercase()),
                    });
                }
            } else {
                // Resolved (>=1 variant): one consume per (method, url) pair that classifies to a key,
                // deduped by key; a pair whose URL variant classifies to nothing (the veto list in
                // `base_relative_path`, etc.) falls back to the unresolved shape above, deduped per
                // method since `raw` is the whole call-arg source text, not per-variant.
                let raw = expr_text(hc.arg, self.cm);
                let mut seen_keys: HashSet<String> = HashSet::new();
                let mut seen_unresolved_methods: HashSet<String> = HashSet::new();
                for method in &hc.methods {
                    for url in &url_variants {
                        match consume_key_for(method, url) {
                            Some(key) => {
                                if seen_keys.insert(key.clone()) {
                                    self.out.push(IoConsume {
                                        kind: "http".into(),
                                        key: Some(key),
                                        file: self.file.into(),
                                        line,
                                        raw: None,
                                        method: None,
                                    });
                                }
                            }
                            None => {
                                if seen_unresolved_methods.insert(method.clone()) {
                                    self.out.push(IoConsume {
                                        kind: "http".into(),
                                        key: None,
                                        file: self.file.into(),
                                        line,
                                        raw: Some(raw.clone()),
                                        method: Some(method.to_uppercase()),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        call.visit_children_with(self); // recurse into nested calls
    }
}

/// Classify one resolved URL variant against one method into its interface key, or `None` if the URL
/// veto-lists out of all three buckets. Internal calls join to a same-repo BE route; external calls get
/// a host-carrying key ("METHOD https://host/path", left verbatim rather than run through
/// `http_interface_key`, which would mangle the origin) so `link_cross_layer_io`'s `"://"` gate routes
/// them external. A base-relative literal (`users/login` — the axios `baseURL` idiom) keys as its
/// root-normalized path; see `base_relative_path`'s doc for the exact veto list.
fn consume_key_for(method: &str, url: &str) -> Option<String> {
    if url.starts_with('/') {
        Some(http_consume_interface_key(method, url))
    } else if is_external(url) {
        Some(format!("{} {}", method.to_uppercase(), url))
    } else {
        base_relative_path(url).map(|rooted| http_consume_interface_key(method, &rooted))
    }
}

fn is_external(u: &str) -> bool {
    let l = u.to_ascii_lowercase();
    l.starts_with("http://") || l.starts_with("https://")
}

/// Public form of the internal/external classification, exported so `zzop-engine`'s late cross-file
/// resolution can apply the same gating (leading `/` = internal, `http(s)://` = external verbatim key,
/// base-relative = root-normalized internal, else unresolved) instead of force-keying every resolved
/// constant as internal.
pub fn is_external_url(u: &str) -> bool {
    is_external(u)
}

/// A base-relative path literal (`users/login`, `articles?limit=10`) — the standard axios/ky idiom of a
/// path resolved against a configured `baseURL` whose value is invisible at the call site (cross-file
/// config or env). Returns the ROOT-NORMALIZED path (`/users/login`): exact when the base carries no
/// path segment, and one prefix dimension off — absorbed by `cross-layer/route-near-miss`'s
/// "missing path prefix" explanation — when it does (decision: cross-layer-resolution, 2026-07-10,
/// pulled by the connected-pair dogfood where 19/19 recognized axios calls fell unresolved).
///
/// NOT base-relative — `None`, left unresolved, never guessed: an empty string, a leading-interpolation
/// template (`{}`-headed — the base itself is the expression), a document-relative `./`/`../` path, a
/// query-only URL (`?page=2` — "same path, new query", which names no path at all), any
/// scheme-carrying URL (`ws://` etc.; `http(s)://` is already the external branch), or
/// whitespace-carrying text (not a path).
pub fn base_relative_path(u: &str) -> Option<String> {
    if u.is_empty()
        || u.starts_with('/')
        || u.starts_with('.')
        || u.starts_with('{')
        || u.starts_with('?')
        || u.contains("://")
        || u.contains(char::is_whitespace)
    {
        return None;
    }
    Some(format!("/{u}"))
}

struct HttpCall<'a> {
    /// One method (the common case) or two, cons-arm first then alt-arm, when the callee was a
    /// computed member with a two-literal ternary bracket expression (`cond-literal-fanout-v1`).
    methods: Vec<String>,
    arg: &'a Expr,
}

fn match_http_call(call: &CallExpr) -> Option<HttpCall<'_>> {
    let arg = &*call.args.first()?.expr;
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    match &**callee {
        Expr::Member(m) => {
            let Expr::Ident(obj) = &*m.obj else {
                return None;
            };
            let obj = obj.sym.to_string();
            match &m.prop {
                MemberProp::Ident(name) => {
                    let name = name.sym.to_string();
                    if obj == "oazapfts" && is_oazapfts_fetch(&name) {
                        let method =
                            method_from_options(call.args.get(1)).unwrap_or_else(|| "GET".into());
                        Some(HttpCall {
                            methods: vec![method],
                            arg,
                        })
                    } else if (obj == "axios" || obj == "ky") && is_http_method(&name) {
                        Some(HttpCall {
                            methods: vec![name],
                            arg,
                        })
                    } else {
                        None
                    }
                }
                // Computed member callee — `axios['post'](url)` / `axios[cond ? 'delete' : 'post'](url)`.
                // Only `axios`/`ky` (never oazapfts, which never spells its verb this way).
                MemberProp::Computed(c) => {
                    if obj != "axios" && obj != "ky" {
                        return None;
                    }
                    let methods = methods_from_computed_prop(&c.expr)?;
                    Some(HttpCall { methods, arg })
                }
                MemberProp::PrivateName(_) => None,
            }
        }
        Expr::Ident(id) => {
            let n = id.sym.to_string();
            if n == "fetch" || n == "$fetch" {
                let method = method_from_options(call.args.get(1)).unwrap_or_else(|| "GET".into());
                Some(HttpCall {
                    methods: vec![method],
                    arg,
                })
            } else if n == "axios" {
                Some(HttpCall {
                    methods: vec!["GET".into()],
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

/// Resolve a computed member-access bracket expression (`axios[<expr>](url)`) to one or two HTTP
/// methods, or `None` if not a recognized shape — never guessed. A bare string literal in the verb set
/// is one method; a ternary whose cons AND alt are BOTH string literals AND BOTH in the verb set is two
/// methods, cons first (`cond-literal-fanout-v1`). An identifier, a literal outside the verb set on
/// either arm, or any other shape (including a one-literal-one-dynamic ternary) rejects the whole call
/// site, matching the "never guess" convention: no method is invented, and no half-known site is
/// silently narrowed to just its literal arm.
fn methods_from_computed_prop(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::Lit(Lit::Str(s)) => {
            let v = s.value.as_str().unwrap_or_default();
            is_http_method(v).then(|| vec![v.to_string()])
        }
        Expr::Cond(c) => {
            let Expr::Lit(Lit::Str(cons)) = &*c.cons else {
                return None;
            };
            let Expr::Lit(Lit::Str(alt)) = &*c.alt else {
                return None;
            };
            let cons_v = cons.value.as_str().unwrap_or_default();
            let alt_v = alt.value.as_str().unwrap_or_default();
            if is_http_method(cons_v) && is_http_method(alt_v) {
                Some(vec![cons_v.to_string(), alt_v.to_string()])
            } else {
                None
            }
        }
        _ => None,
    }
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

/// Resolve a URL argument to every syntactically-possible path string ("variants"); an empty vec means
/// dynamic/unresolvable, same meaning as the old `None`. A plain literal or const indirection yields
/// exactly one variant, unchanged from before. A top-level ternary whose BOTH arms are string literals
/// fans out to one variant per arm (cons first, then alt), deduped preserving first-seen order — visible
/// literal enumeration, not a guess (`cond-literal-fanout-v1`); any other ternary shape is unresolved
/// (empty vec), same as today's non-literal shapes. See [`resolve_template_variants`] for the template
/// literal case, which fans out per-interpolation with its own cap.
fn resolve_url_variants(
    arg: &Expr,
    consts: &HashMap<String, String>,
    cm: &SourceMap,
) -> Vec<String> {
    match arg {
        Expr::Lit(Lit::Str(s)) => vec![s.value.as_str().unwrap_or_default().to_string()],
        Expr::Cond(c) => {
            let Expr::Lit(Lit::Str(cons)) = &*c.cons else {
                return Vec::new();
            };
            let Expr::Lit(Lit::Str(alt)) = &*c.alt else {
                return Vec::new();
            };
            dedup_preserve_order(vec![
                cons.value.as_str().unwrap_or_default().to_string(),
                alt.value.as_str().unwrap_or_default().to_string(),
            ])
        }
        Expr::Tpl(t) => resolve_template_variants(t, cm),
        Expr::Ident(_) | Expr::Member(_) => consts
            .get(&expr_text(arg, cm))
            .cloned()
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// One piece of a template literal between two quasis: either fixed text (the old `"{}"` placeholder,
/// or an empty string for a dropped trailing QS suffix), or a conditional-literal fan-out slot carrying
/// its two literal arm values.
enum TplPiece {
    Fixed(String),
    Slot(String, String),
}

/// Template literal -> URL variants. `/api/users/${id}` -> `["/api/users/{}"]`, same as before. A
/// *trailing* interpolation starting with `QS.` is still oazapfts's query-string suffix, dropped
/// entirely rather than turned into `{}`. NEW (`cond-literal-fanout-v1`): an interpolation whose
/// expression is a ternary with BOTH arms string literals is a fan-out slot instead of a fixed `{}` —
/// e.g. `` `/users${isRegister ? '' : '/login'}` `` -> `["/users", "/users/login"]`. Multiple slots
/// cartesian-product together, capped at 2 slots (<=4 variants); a 3rd+ slot forces EVERY slot in this
/// template back to the old fixed `{}` behavior, keeping output bounded and deterministic. Variants are
/// deduped preserving first-seen order.
fn resolve_template_variants(t: &Tpl, cm: &SourceMap) -> Vec<String> {
    let last_idx = t.exprs.len().checked_sub(1);

    let mut pieces: Vec<TplPiece> = Vec::with_capacity(t.exprs.len());
    let mut slot_count = 0usize;
    for (i, e) in t.exprs.iter().enumerate() {
        let is_trailing = last_idx == Some(i)
            && t.quasis
                .get(i + 1)
                .and_then(|q| q.cooked.as_ref())
                .map(|a| a.as_str().unwrap_or_default().is_empty())
                .unwrap_or(false);
        let is_qs_suffix = is_trailing && expr_text(e, cm).starts_with("QS.");
        if is_qs_suffix {
            pieces.push(TplPiece::Fixed(String::new()));
            continue;
        }
        if let Expr::Cond(c) = &**e {
            if let (Expr::Lit(Lit::Str(cons)), Expr::Lit(Lit::Str(alt))) = (&*c.cons, &*c.alt) {
                slot_count += 1;
                pieces.push(TplPiece::Slot(
                    cons.value.as_str().unwrap_or_default().to_string(),
                    alt.value.as_str().unwrap_or_default().to_string(),
                ));
                continue;
            }
        }
        pieces.push(TplPiece::Fixed("{}".to_string()));
    }
    if slot_count > 2 {
        // Bounded-output cap: fall back to fixed "{}" for every slot in this template.
        for p in &mut pieces {
            if matches!(p, TplPiece::Slot(_, _)) {
                *p = TplPiece::Fixed("{}".to_string());
            }
        }
    }

    let mut variants = vec![String::new()];
    for (i, q) in t.quasis.iter().enumerate() {
        let quasi_text = q
            .cooked
            .as_ref()
            .and_then(|a| a.as_str())
            .unwrap_or_default();
        for v in variants.iter_mut() {
            v.push_str(quasi_text);
        }
        if i < t.exprs.len() {
            match &pieces[i] {
                TplPiece::Fixed(s) => {
                    for v in variants.iter_mut() {
                        v.push_str(s);
                    }
                }
                TplPiece::Slot(cons, alt) => {
                    let mut next = Vec::with_capacity(variants.len() * 2);
                    for v in &variants {
                        let mut a = v.clone();
                        a.push_str(cons);
                        next.push(a);
                        let mut b = v.clone();
                        b.push_str(alt);
                        next.push(b);
                    }
                    variants = next;
                }
            }
        }
    }
    dedup_preserve_order(variants)
}

/// Dedup a variant list, preserving first-seen order (`cond-literal-fanout-v1`'s "same literal on both
/// arms" and "same variant produced twice" cases must collapse to one).
fn dedup_preserve_order(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
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
/// `const <Name> = { ... }` object literal PLUS every top-level (incl. `export`) `enum` whose member
/// initializers are string literals (`RouteKey.Asset -> "assets"`) in this file's text alone. A member
/// with a numeric, implicit (auto-incrementing), or computed initializer is skipped — never guessed.
/// `build_const_map` folds this over every file; a caller with only one file in hand can merge fragments
/// later and re-resolve via [`resolve_raw_path`].
///
/// This map feeds TWO assemble-time consumers: [`resolve_raw_path`]'s late cross-file CONSUME
/// re-resolution, and (new) `zzop_engine::analyze::compose`'s controller-prefix PROVIDE resolution —
/// see `zzop_core::ControllerPrefixRouteFragment`'s doc for the `@Controller(RouteKey.Asset)` shape this
/// unblocks.
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
        if let Some(Decl::TsEnum(e)) = decl {
            let enum_name = e.id.sym.to_string();
            for member in &e.members {
                let member_name = match &member.id {
                    TsEnumMemberId::Ident(id) => id.sym.to_string(),
                    TsEnumMemberId::Str(s) => s.value.as_str().unwrap_or_default().to_string(),
                };
                // A numeric or implicit (no initializer at all — swc's auto-increment) member is
                // skipped: only a STRING literal initializer is a resolvable route-prefix constant.
                let Some(init) = &member.init else { continue };
                if let Expr::Lit(Lit::Str(s)) = unwrap_expr(init) {
                    map.insert(
                        format!("{enum_name}.{member_name}"),
                        s.value.as_str().unwrap_or_default().to_string(),
                    );
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

    // --- base-relative path literals (`base-relative-egress-v1`) ---

    #[test]
    fn base_relative_literal_keys_as_its_root_normalized_path() {
        // The axios `baseURL` idiom: paths with no leading slash, resolved against a configured base
        // invisible at the call site. Keyed root-normalized; a base prefix (`/api`) is then a
        // route-near-miss "missing path prefix" away, not an unexplained unresolved.
        let out = extract_http_egress(&files(&[(
            "conduit.ts",
            "axios.post('users/login', body); axios.get(`articles/${slug}/comments`); axios.get('articles?limit=10');",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("POST /users/login".to_string()),
                Some("GET /articles/{}/comments".to_string()),
                // Query suffix dropped (`query-drop-v1`) — a route provide never carries one, so
                // `GET /articles?limit=10` could never join `GET /articles` nor be explained by
                // route-near-miss's segment comparison (dogfood round 6: 2/19 corpus consumes
                // were query-suffixed and silently unexplainable).
                Some("GET /articles".to_string()),
            ]
        );
        assert!(out.iter().all(|c| c.raw.is_none()));
    }

    #[test]
    fn interpolated_query_suffix_is_dropped_from_the_key() {
        // `` `articles?${qs}` `` resolves to `articles?{}` — the query drop must also cover the
        // interpolated form, on both rooted and base-relative paths.
        let out = extract_http_egress(&files(&[(
            "q.ts",
            "axios.get(`articles?${qs}`); axios.get(`/articles/feed?${qs}`);",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("GET /articles".to_string()),
                Some("GET /articles/feed".to_string()),
            ]
        );
    }

    #[test]
    fn base_relative_veto_list_still_never_keys() {
        // Leading-interpolation template (the base itself is the expression), document-relative `./`,
        // query-only URL, non-http scheme, whitespace — all stay unresolved with raw+method carried.
        let out = extract_http_egress(&files(&[(
            "v.ts",
            "axios.get(`${API_ROOT}${url}`); axios.get('./users'); axios.get('?page=2'); axios.get('ws://host/ch'); axios.get('not a path');",
        )]));
        assert_eq!(out.len(), 5);
        assert!(
            out.iter().all(|c| c.key.is_none() && c.method.is_some()),
            "got: {:?}",
            keys(&out)
        );
    }

    // --- conditional-literal fan-out (`cond-literal-fanout-v1`) ---

    #[test]
    fn template_conditional_literal_interpolation_fans_out_the_url() {
        let out = extract_http_egress(&files(&[(
            "conduit.ts",
            "axios.post(`/users${isRegister ? '' : '/login'}`, body);",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("POST /users".to_string()),
                Some("POST /users/login".to_string()),
            ]
        );
        assert!(out.iter().all(|c| c.raw.is_none() && c.method.is_none()));
    }

    #[test]
    fn computed_member_ternary_callee_fans_out_the_method() {
        let out = extract_http_egress(&files(&[(
            "conduit.ts",
            "axios[favorited ? 'delete' : 'post'](`/articles/${slug}/favorite`);",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("DELETE /articles/{}/favorite".to_string()),
                Some("POST /articles/{}/favorite".to_string()),
            ]
        );
    }

    #[test]
    fn computed_member_string_literal_callee_is_a_single_method() {
        let out = extract_http_egress(&files(&[("a.ts", "axios['post']('/a');")]));
        assert_eq!(keys(&out), vec![Some("POST /a".to_string())]);
    }

    #[test]
    fn computed_member_identifier_callee_is_not_recognized() {
        let out = extract_http_egress(&files(&[("a.ts", "axios[verb]('/a');")]));
        assert!(out.is_empty());
    }

    #[test]
    fn computed_member_ternary_with_an_arm_outside_the_verb_set_rejects_the_whole_site() {
        // `head` is not a recognized verb — one bad arm rejects the site entirely rather than silently
        // narrowing to just the `get` arm (never guess).
        let out = extract_http_egress(&files(&[("a.ts", "axios[cond ? 'get' : 'head']('/a');")]));
        assert!(out.is_empty());
    }

    #[test]
    fn top_level_ternary_url_argument_fans_out() {
        let out = extract_http_egress(&files(&[("a.ts", "axios.get(cond ? '/a' : '/b');")]));
        assert_eq!(
            keys(&out),
            vec![Some("GET /a".to_string()), Some("GET /b".to_string())]
        );
    }

    #[test]
    fn template_ternary_interpolation_with_one_non_literal_arm_keeps_the_old_placeholder() {
        let out = extract_http_egress(&files(&[("a.ts", "axios.get(`/x${cond ? a : 'b'}`);")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /x{}"));
    }

    #[test]
    fn more_than_two_conditional_literal_interpolations_falls_back_to_placeholders_for_all() {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "axios.get(`/x${a ? '1' : '2'}/y${b ? '3' : '4'}/z${c ? '5' : '6'}`);",
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /x{}/y{}/z{}"));
    }

    #[test]
    fn top_level_ternary_with_identical_arms_dedups_to_one_consume() {
        let out = extract_http_egress(&files(&[("a.ts", "axios.get(cond ? '/same' : '/same');")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /same"));
    }

    #[test]
    fn ternary_with_one_keying_and_one_vetoed_arm_emits_the_key_plus_an_unresolved_consume() {
        // Mixed partial-veto: '/feed' keys, '?public' veto-lists out of every bucket (query-only URL
        // names no path). The keyed variant is emitted AND the vetoed variant falls back to the
        // unresolved shape (raw = the whole ternary's source text, method carried) — strictly additive
        // over the pre-fanout behavior, which emitted only the unresolved consume.
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "axios.get(cond ? '/feed' : '?public');",
        )]));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].key.as_deref(), Some("GET /feed"));
        assert!(out[0].raw.is_none());
        assert!(out[1].key.is_none());
        assert_eq!(out[1].raw.as_deref(), Some("cond ? '/feed' : '?public'"));
        assert_eq!(out[1].method.as_deref(), Some("GET"));
    }

    #[test]
    fn computed_member_ternary_callee_with_unresolved_url_carries_both_methods() {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "axios[cond ? 'delete' : 'post'](buildUrl(x));",
        )]));
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|c| c.key.is_none()));
        assert!(out.iter().all(|c| c.raw.as_deref() == Some("buildUrl(x)")));
        assert_eq!(
            out.iter().map(|c| c.method.clone()).collect::<Vec<_>>(),
            vec![Some("DELETE".to_string()), Some("POST".to_string())]
        );
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

    // --- enum members joining the const map (controller-prefix-ref-v1) ---

    #[test]
    fn exported_string_enum_member_joins_the_const_map() {
        let frag = const_map_fragment(
            "enum.ts",
            "export enum RouteKey { Asset = 'assets', User = 'users' }\n",
        );
        assert_eq!(
            frag.get("RouteKey.Asset").map(String::as_str),
            Some("assets")
        );
        assert_eq!(frag.get("RouteKey.User").map(String::as_str), Some("users"));
    }

    #[test]
    fn non_exported_string_enum_member_joins_the_const_map_too() {
        let frag = const_map_fragment("enum.ts", "enum RouteKey { Asset = 'assets' }\n");
        assert_eq!(
            frag.get("RouteKey.Asset").map(String::as_str),
            Some("assets")
        );
    }

    #[test]
    fn numeric_enum_member_is_skipped_not_guessed() {
        let frag = const_map_fragment("enum.ts", "enum Level { Low = 0, High = 1 }\n");
        assert!(
            frag.is_empty(),
            "numeric initializers must never join the const map: {frag:?}"
        );
    }

    #[test]
    fn implicit_auto_increment_enum_member_is_skipped_not_guessed() {
        let frag = const_map_fragment("enum.ts", "enum Level { Low, High }\n");
        assert!(
            frag.is_empty(),
            "a member with no initializer at all must never guess a value: {frag:?}"
        );
    }

    #[test]
    fn mixed_string_and_numeric_enum_members_only_joins_the_string_ones() {
        let frag = const_map_fragment(
            "enum.ts",
            "export enum Mixed { Path = 'x', Count = 1, Auto }\n",
        );
        assert_eq!(frag.len(), 1);
        assert_eq!(frag.get("Mixed.Path").map(String::as_str), Some("x"));
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
