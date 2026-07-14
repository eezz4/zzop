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
//! `fetch(url, { method })`, `$fetch(url, { method })`, `axios(url)`, and a computed member callee on
//! `axios`/`ky` (`axios['post'](url)`, `axios[favorited ? 'delete' : 'post'](url)`) whose bracket
//! expression is a recognized-verb string literal or a ternary with two such literal arms, and
//! Angular's `this.<name>.get/post/put/delete/patch(url)` / `<name>.get/...(url)` where `<name>` must be
//! HttpClient-typed (constructor param property or class property) or `inject(HttpClient)`-initialized
//! in a file that itself imports `@angular/common/http` (see `angular-httpclient-v1` below).
//! Generated-SDK clients (e.g. oazapfts) are NOT recognized here — that vocabulary lives in an
//! injection adapter (`examples/oazapfts-adapter`), not the engine (decision: generated SDKs are
//! injection adapters, not engine vocab).
//!
//! `cond-literal-fanout-v1`: a ternary with two string-literal arms — as the whole URL argument, as a
//! template interpolation, or as the computed-member method — enumerates one deterministic key per arm
//! instead of collapsing to `{}`/going unrecognized. Both arms are visible literals in the source, so
//! this is normalization of visible facts, not speculation (the "never guess" convention only forbids
//! inventing values that aren't written down). Template fan-out is capped at 2 conditional-literal
//! interpolations (≤4 variants); a 3rd+ interpolation of that shape falls back to `{}` for ALL of them
//! in that template, keeping output bounded and deterministic.
//!
//! `angular-httpclient-v1`: Angular's dependency-injected `HttpClient` idiom — `this.<name>.get/post/
//! put/delete/patch(url)` or `<name>.get/...(url)` — is recognized only when `<name>` is a proven
//! HttpClient receiver in THIS file: a constructor parameter property typed `HttpClient`, a class
//! property typed `HttpClient`, or a class property/local `const`/`let` initialized with
//! `inject(HttpClient)`, gated on the file itself importing (any specifier) from
//! `@angular/common/http` — never guessed from the bare name `http` alone. Over-approximation WITHIN a
//! gated file is accepted: resolution is per-file, not per-class, so two same-named-but-differently-typed
//! receivers in one gated file both match. `request(method, url)` is out of scope for v1.
//!
//! `str-concat-url-v1`: binary `+` string concatenation (`'/profiles/' + username`, `'/profiles/' +
//! username + '/follow'`) is the isomorphic counterpart to template-literal resolution. The
//! left-associative `+` chain is flattened into its operands; the whole chain is rejected (unresolved)
//! if any operator in it is not `+` (a `-`/`??`/`||` chain is never guessed) or if NO operand is a direct
//! string literal (a fully-dynamic `base + path` stays unresolved, same as today). Each operand maps to
//! the same `TplPiece` vocabulary as template resolution: a string literal or resolved const is `Fixed`;
//! a ternary with two string-literal arms is a `Slot` (cartesian fan-out, capped at 2 slots — same
//! bounded-output rule as `cond-literal-fanout-v1`); anything else falls back to the old `{}`
//! placeholder.

use std::collections::{HashMap, HashSet};

use swc_core::common::{SourceMap, SourceMapper, Spanned};
use swc_core::ecma::ast::{
    BinaryOp, CallExpr, Callee, ClassProp, Constructor, Decl, Expr, ExprOrSpread, Lit, MemberProp,
    Module, ModuleDecl, ModuleItem, ObjectLit, ParamOrTsParamProp, Pat, Prop, PropName,
    PropOrSpread, Stmt, Tpl, TsEntityName, TsEnumMemberId, TsParamPropParam, TsType, TsTypeAnn,
    VarDecl, VarDeclKind,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_consume_interface_key, ConsumeBodyShape, IoConsume};

/// Extract HTTP egress IoConsume entries across all files (the const map is project-wide).
pub fn extract_http_egress(files: &[(String, String)]) -> Vec<IoConsume> {
    let consts = build_const_map(files);
    let mut out = Vec::new();
    for (rel, text) in files {
        let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
            continue;
        };
        let cm_ref: &SourceMap = &cm;
        let angular_receivers = angular_http_client_receivers(&module);
        let mut c = EgressCollector {
            cm: cm_ref,
            file: rel,
            consts: &consts,
            angular_receivers: &angular_receivers,
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
    angular_receivers: &'a HashSet<String>,
    out: Vec<IoConsume>,
}

impl Visit for EgressCollector<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Some(hc) =
            match_http_call(call).or_else(|| match_angular_http_call(call, self.angular_receivers))
        {
            // Body-shape evidence is a property of THIS call site (its `args[1]`), independent of which
            // method/URL variant a given emitted IoConsume ends up carrying — computed once and cloned
            // into every emit point below (resolved/unresolved/vetoed alike), per `body-shape-v1`.
            let body = witnessed_body_shape(call, &hc);
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
                        client: Some(hc.client.to_string()),
                        body: body.clone(),
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
                                        client: Some(hc.client.to_string()),
                                        body: body.clone(),
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
                                        client: Some(hc.client.to_string()),
                                        body: body.clone(),
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
///
/// Fourth bucket, checked after the three above: **base-carrier head-drop**. A template/concat that
/// assembles to a single leading `{}` immediately followed by a `/`-headed literal (`"{}/me/achievements"`
/// from `` fetch(`${BASE_URL}/me/achievements`) ``) has an opaque, cross-file-invisible base carried by
/// that one interpolation slot — mirroring the base-relative decision above, the base is DROPPED, never
/// valued, and the visible `/`-headed literal keys the call. This is a narrow near-miss prefix class: it
/// only fires when exactly one `{}` leads and what follows starts with `/` but not `//` (a second `{}`
/// right after the head, a non-`/` literal suffix, or a `//` post-drop head all stay unresolved — see
/// `base_relative_path`'s doc for why those shapes are never-guess residue).
fn consume_key_for(method: &str, url: &str) -> Option<String> {
    if url.starts_with('/') {
        Some(http_consume_interface_key(method, url))
    } else if is_external(url) {
        Some(format!("{} {}", method.to_uppercase(), url))
    } else if let Some(rest) = url
        .strip_prefix("{}")
        .filter(|rest| rest.starts_with('/') && !rest.starts_with("//"))
    {
        Some(http_consume_interface_key(method, rest))
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
/// whitespace-carrying text (not a path). The `{}`-headed veto here is still correct for the general case
/// (the base itself is an opaque expression with no visible path), but ONE sub-shape of it — exactly one
/// leading `{}` immediately followed by a `/`-headed literal — is keyed upstream in `consume_key_for`
/// (base-carrier head-drop) before this function ever sees it; the veto here still catches everything
/// that shape excludes (`{}{}...`, `{}non-slash`, `{}//host/...`).
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
    /// How `call.args.get(1)` maps to a request body, for `witnessed_body_shape` — set per matched
    /// call shape (`body-shape-v1`).
    body_style: BodyStyle,
    /// Which client recognizer matched this call site (`axios-defaults-base-v1`) — carried onto every
    /// `IoConsume` this call site emits as `IoConsume::client`, so a client-scoped normalization seam
    /// (e.g. `axios.defaults.baseURL`) can tell an axios consume from a fetch/ky one in the same tree.
    client: &'static str,
}

/// How the second call argument relates to a request body, at a matched HTTP call site
/// (`body-shape-v1`). See `witnessed_body_shape`'s doc for how each style is read.
#[derive(Clone, Copy)]
enum BodyStyle {
    /// `args[1]` (if present) IS the body value directly — the axios/ky/Angular-HttpClient idiom.
    /// Only actually read when every resolved method is a body-position verb (`is_body_position_verb`).
    DirectArg,
    /// `args[1]` is an options object; the body lives at its own `body:` property — the bare
    /// `fetch`/`$fetch` idiom (mirrors how `method_from_options` reads `method:` from the same
    /// object).
    OptionsBodyProp,
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
                    if (obj == "axios" || obj == "ky") && is_http_method(&name) {
                        Some(HttpCall {
                            methods: vec![name],
                            arg,
                            body_style: BodyStyle::DirectArg,
                            client: if obj == "axios" { "axios" } else { "ky" },
                        })
                    } else {
                        None
                    }
                }
                // Computed member callee — `axios['post'](url)` / `axios[cond ? 'delete' : 'post'](url)`.
                // Only `axios`/`ky`.
                MemberProp::Computed(c) => {
                    if obj != "axios" && obj != "ky" {
                        return None;
                    }
                    let methods = methods_from_computed_prop(&c.expr)?;
                    Some(HttpCall {
                        methods,
                        arg,
                        body_style: BodyStyle::DirectArg,
                        client: if obj == "axios" { "axios" } else { "ky" },
                    })
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
                    body_style: BodyStyle::OptionsBodyProp,
                    client: if n == "fetch" { "fetch" } else { "$fetch" },
                })
            } else if n == "axios" {
                Some(HttpCall {
                    methods: vec!["GET".into()],
                    arg,
                    body_style: BodyStyle::DirectArg,
                    client: "axios",
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Whether `m` is a lowercase spelling of a `zzop_core::HTTP_KEY_VERBS` verb — the member-callee /
/// computed-member vocabulary (T1: the verb SET lives in core; the exact-lowercase comparison is this
/// vocabulary's own spelling rule, so `axios.GET(...)` — not a real client API — stays unrecognized).
fn is_http_method(m: &str) -> bool {
    zzop_core::HTTP_KEY_VERBS
        .iter()
        .any(|v| v.to_ascii_lowercase() == m)
}

/// The body-position HTTP verb set (`body-shape-v1`) — a `BodyStyle::DirectArg` call's `args[1]` is only
/// treated as a request body when EVERY resolved method is one of these (a `GET`/`DELETE`/`HEAD` verb's
/// second argument is a config/options object, never a body; a mixed-verb computed-member fanout with
/// even ONE non-body-position verb rejects the whole site rather than guessing). Uppercase, matching
/// `zzop_core::HTTP_KEY_VERBS`'s spelling; comparisons uppercase the candidate first since `HttpCall::methods`
/// mixes lowercase (axios/ky/Angular verb-name spellings) and uppercase (fetch/`$fetch`
/// `method_from_options` output) depending on which matcher produced it.
///
/// Do not unify with `zzop_rules_http`'s `WRITE_HTTP_METHODS` (policy inventory T3): that set is a
/// WRITE-SEMANTICS vocabulary and includes DELETE; this one is an axios/ky/Angular CALL-SIGNATURE
/// fact (`.delete(url, config)` puts a config object at `args[1]`, never a body), so the DELETE
/// divergence is the point.
const BODY_POSITION_VERBS: &[&str] = &["POST", "PUT", "PATCH"];

/// Whether `m` (any case) names a [`BODY_POSITION_VERBS`] member.
fn is_body_position_verb(m: &str) -> bool {
    BODY_POSITION_VERBS.contains(&m.to_ascii_uppercase().as_str())
}

/// Angular HttpClient call matcher (`angular-httpclient-v1`) — `this.<name>.<verb>(url, ...)` or
/// `<name>.<verb>(url, ...)` where `<name>` is a proven HttpClient receiver (see module doc). Sibling to
/// [`match_http_call`], never called when `receivers` is empty (the file didn't import
/// `@angular/common/http`, or nothing in it resolved as an HttpClient receiver).
fn match_angular_http_call<'a>(
    call: &'a CallExpr,
    receivers: &HashSet<String>,
) -> Option<HttpCall<'a>> {
    if receivers.is_empty() {
        return None;
    }
    let arg = &*call.args.first()?.expr;
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(outer) = &**callee else {
        return None;
    };
    let MemberProp::Ident(verb_ident) = &outer.prop else {
        return None;
    };
    let verb = verb_ident.sym.to_string();
    if !is_http_method(&verb) {
        return None;
    }
    let receiver_name = match &*outer.obj {
        // `this.<name>.<verb>(...)`
        Expr::Member(inner) => {
            if !matches!(&*inner.obj, Expr::This(_)) {
                return None;
            }
            let MemberProp::Ident(name_ident) = &inner.prop else {
                return None;
            };
            name_ident.sym.to_string()
        }
        // `<name>.<verb>(...)` — a field-inject or local-const-inject receiver referenced bare.
        Expr::Ident(id) => id.sym.to_string(),
        _ => return None,
    };
    if !receivers.contains(&receiver_name) {
        return None;
    }
    Some(HttpCall {
        methods: vec![verb],
        arg,
        body_style: BodyStyle::DirectArg,
        client: "angular",
    })
}

/// True when the file imports (any specifier) from `@angular/common/http` — the hard per-file evidence
/// gate for [`match_angular_http_call`]; see module doc (`angular-httpclient-v1`).
fn has_angular_http_client_import(module: &Module) -> bool {
    module.body.iter().any(|item| {
        matches!(
            item,
            ModuleItem::ModuleDecl(ModuleDecl::Import(imp))
                if imp.src.value.as_str() == Some("@angular/common/http")
        )
    })
}

/// This file's set of proven Angular HttpClient receiver names — empty unless the file imports
/// `@angular/common/http` (module doc). Three shapes contribute: a constructor parameter property typed
/// `HttpClient`, a class property typed `HttpClient` or initialized with `inject(HttpClient)`, and a
/// top-level or function-local `const`/`let` initialized with `inject(HttpClient)`. Resolution walks the
/// WHOLE tree (not just top-level), so a nested method-local `inject(HttpClient)` const is found too.
fn angular_http_client_receivers(module: &Module) -> HashSet<String> {
    if !has_angular_http_client_import(module) {
        return HashSet::new();
    }
    let mut c = HttpClientReceiverCollector {
        names: HashSet::new(),
    };
    module.visit_with(&mut c);
    c.names
}

struct HttpClientReceiverCollector {
    names: HashSet<String>,
}

impl Visit for HttpClientReceiverCollector {
    fn visit_constructor(&mut self, n: &Constructor) {
        for p in &n.params {
            if let ParamOrTsParamProp::TsParamProp(tpp) = p {
                if let TsParamPropParam::Ident(bi) = &tpp.param {
                    if is_http_client_type(bi.type_ann.as_deref()) {
                        self.names.insert(bi.id.sym.to_string());
                    }
                }
            }
        }
        n.visit_children_with(self);
    }

    fn visit_class_prop(&mut self, n: &ClassProp) {
        if let PropName::Ident(key) = &n.key {
            let is_http_client = is_http_client_type(n.type_ann.as_deref())
                || n.value.as_deref().is_some_and(is_inject_http_client_call);
            if is_http_client {
                self.names.insert(key.sym.to_string());
            }
        }
        n.visit_children_with(self);
    }

    fn visit_var_decl(&mut self, n: &VarDecl) {
        if matches!(n.kind, VarDeclKind::Const | VarDeclKind::Let) {
            for d in &n.decls {
                if let (Pat::Ident(bi), Some(init)) = (&d.name, d.init.as_deref()) {
                    if is_inject_http_client_call(init) {
                        self.names.insert(bi.id.sym.to_string());
                    }
                }
            }
        }
        n.visit_children_with(self);
    }
}

/// `: HttpClient` type annotation — a single-identifier `TsTypeRef` named exactly `HttpClient`.
fn is_http_client_type(ann: Option<&TsTypeAnn>) -> bool {
    let Some(ann) = ann else { return false };
    matches!(&*ann.type_ann, TsType::TsTypeRef(tr) if matches!(&tr.type_name, TsEntityName::Ident(id) if id.sym == "HttpClient"))
}

/// `inject(HttpClient)` — callee identifier `inject`, exactly one argument, that argument the bare
/// identifier `HttpClient`.
fn is_inject_http_client_call(e: &Expr) -> bool {
    let Expr::Call(call) = e else { return false };
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Ident(id) = &**callee else {
        return false;
    };
    id.sym == "inject"
        && call.args.len() == 1
        && matches!(&*call.args[0].expr, Expr::Ident(arg) if arg.sym == "HttpClient")
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

/// Read `{ method: "post" }` from a fetch/axios options object. Only a literal `method: "..."` key is
/// read; `...opts` spreads are silently skipped.
fn method_from_options(opts: Option<&ExprOrSpread>) -> Option<String> {
    let expr = &*opts?.expr;
    let Expr::Object(obj) = expr else {
        return None;
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

// --- Request-body shape extraction (`body-shape-v1`) ---

/// Extract the statically witnessed request-body shape at a matched HTTP call site, or `None` when no
/// static object literal is visible at the expected position — evidence-only, never guessed (module doc
/// / `zzop_core::ConsumeBodyShape`'s doc). Reads `call.args.get(1)` differently depending on `hc.body_style`:
/// - [`BodyStyle::DirectArg`]: `args[1]` IS the body, gated to sites where EVERY resolved method is a
///   [`BODY_POSITION_VERBS`] member (a GET/DELETE/HEAD second argument is a config object, never a body;
///   a mixed-verb fanout with even one non-body-position verb never guesses which arm the object belongs
///   to, so the whole site yields `None`). A missing `args[1]` (`axios.post(url)`) also yields `None` —
///   an interceptor may inject a body invisibly; absence is not evidence either way.
/// - [`BodyStyle::OptionsBodyProp`]: `args[1]` is an options object; the body lives at that object's own
///   `body:` property, which may be a direct object literal, `JSON.stringify(<object literal>)`, or
///   anything else (`None`).
fn witnessed_body_shape(call: &CallExpr, hc: &HttpCall<'_>) -> Option<ConsumeBodyShape> {
    match hc.body_style {
        BodyStyle::DirectArg => {
            if !hc.methods.iter().all(|m| is_body_position_verb(m)) {
                return None;
            }
            let arg = unwrap_expr(&call.args.get(1)?.expr);
            let Expr::Object(obj) = arg else {
                return None;
            };
            Some(shape_from_object_lit(obj))
        }
        BodyStyle::OptionsBodyProp => {
            let body_expr = unwrap_expr(body_prop_from_options(call.args.get(1))?);
            match body_expr {
                Expr::Object(obj) => Some(shape_from_object_lit(obj)),
                Expr::Call(c) if is_json_stringify_call(c) => {
                    let inner = unwrap_expr(&c.args.first()?.expr);
                    let Expr::Object(obj) = inner else {
                        return None;
                    };
                    Some(shape_from_object_lit(obj))
                }
                _ => None,
            }
        }
    }
}

/// Read the `body:` property's value expression from a fetch/axios options object. Only a literal
/// `body:` key (ident or string prop name) is read; `...opts` spreads are silently skipped (never
/// guessed).
fn body_prop_from_options(opts: Option<&ExprOrSpread>) -> Option<&Expr> {
    let expr = &*opts?.expr;
    let Expr::Object(obj) = expr else {
        return None;
    };
    for prop in &obj.props {
        if let PropOrSpread::Prop(p) = prop {
            if let Prop::KeyValue(kv) = &**p {
                if let Some(name) = prop_name_str(&kv.key) {
                    if name == "body" {
                        return Some(&kv.value);
                    }
                }
            }
        }
    }
    None
}

/// `JSON.stringify(<expr>)` — callee must be exactly the member `JSON.stringify` (receiver matched
/// tightly, never a bare-name allowlist).
fn is_json_stringify_call(call: &CallExpr) -> bool {
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Member(m) = &**callee else {
        return false;
    };
    matches!(&*m.obj, Expr::Ident(id) if id.sym == "JSON")
        && matches!(&m.prop, MemberProp::Ident(name) if name.sym == "stringify")
}

/// A property name statically readable from a `PropName` — `Ident`/`Str` only; `Computed` (and any
/// other future variant) returns `None`, which the caller treats as evidence the enclosing level is
/// incomplete (never guessed).
fn prop_name_str(name: &PropName) -> Option<String> {
    match name {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}

/// Walk ONE ObjectLit's own props (never recursing — the caller decides whether/how to descend),
/// pushing each witnessed key into `keys` (dotted under `prefix` when non-empty, bare otherwise).
/// Returns whether this level is fully static: `true` iff every prop is a plain `Prop::KeyValue`
/// (non-computed key) or `Prop::Shorthand` — a spread, computed key, getter, setter, method, or assign
/// prop makes this level incomplete (its sibling keys are still recorded; only the level's OWN
/// completeness is affected), per `ConsumeBodyShape::complete_at`'s contract.
fn collect_level(obj: &ObjectLit, prefix: &str, keys: &mut Vec<String>) -> bool {
    let mut complete = true;
    for prop in &obj.props {
        match prop {
            PropOrSpread::Spread(_) => complete = false,
            PropOrSpread::Prop(p) => match &**p {
                Prop::Shorthand(ident) => keys.push(dotted(prefix, &ident.sym)),
                Prop::KeyValue(kv) => match prop_name_str(&kv.key) {
                    Some(name) => keys.push(dotted(prefix, &name)),
                    None => complete = false, // PropName::Computed
                },
                Prop::Getter(_) | Prop::Setter(_) | Prop::Method(_) | Prop::Assign(_) => {
                    complete = false;
                }
            },
        }
    }
    complete
}

fn dotted(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

/// Build the statically witnessed [`ConsumeBodyShape`] for a request-body `ObjectLit`, capped at depth 2
/// (the top level, plus one level under each top-level key whose value is itself an object literal —
/// see the type's doc). The top level's own completeness feeds `complete_at: [""]`; each depth-1
/// `KeyValue` whose value is an `ObjectLit` is walked exactly one more level (its keys recorded as
/// `"parent.child"`), and `"parent"` is added to `complete_at` too when THAT level is fully static. A
/// depth-2 nested object is recorded as a single key (its own dotted path) and never descended into —
/// `collect_level` only reads prop NAMES, never recurses on their values — so nothing past depth 2 is
/// ever produced, and a depth-2 path never enters `complete_at` (only `""` and depth-1 paths can).
fn shape_from_object_lit(obj: &ObjectLit) -> ConsumeBodyShape {
    let mut keys = Vec::new();
    let top_complete = collect_level(obj, "", &mut keys);
    let mut complete_at = Vec::new();
    if top_complete {
        complete_at.push(String::new());
    }
    for prop in &obj.props {
        if let PropOrSpread::Prop(p) = prop {
            if let Prop::KeyValue(kv) = &**p {
                if let Some(name) = prop_name_str(&kv.key) {
                    if let Expr::Object(nested) = unwrap_expr(&kv.value) {
                        let nested_complete = collect_level(nested, &name, &mut keys);
                        if nested_complete {
                            complete_at.push(name);
                        }
                    }
                }
            }
        }
    }
    keys.sort();
    keys.dedup();
    complete_at.sort();
    complete_at.dedup();
    ConsumeBodyShape { keys, complete_at }
}

/// Resolve a URL argument to every syntactically-possible path string ("variants"); an empty vec means
/// dynamic/unresolvable, same meaning as the old `None`. A plain literal or const indirection yields
/// exactly one variant, unchanged from before. A top-level ternary whose BOTH arms are string literals
/// fans out to one variant per arm (cons first, then alt), deduped preserving first-seen order — visible
/// literal enumeration, not a guess (`cond-literal-fanout-v1`); any other ternary shape is unresolved
/// (empty vec), same as today's non-literal shapes. See [`resolve_template_variants`] for the template
/// literal case, which fans out per-interpolation with its own cap, and [`resolve_concat_variants`] for
/// the binary `+` string-concatenation case (`str-concat-url-v1`).
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
        Expr::Tpl(t) => resolve_template_variants(t),
        Expr::Ident(_) | Expr::Member(_) => consts
            .get(&expr_text(arg, cm))
            .cloned()
            .into_iter()
            .collect(),
        Expr::Bin(_) => resolve_concat_variants(arg, consts, cm),
        _ => Vec::new(),
    }
}

/// `+` string-concatenation -> URL variants (`str-concat-url-v1`), the isomorphic counterpart to
/// [`resolve_template_variants`] for binary `+` instead of a template literal. Flattens the
/// left-associative chain, rejects it (empty vec) if any operator isn't `+` or if no operand is a direct
/// string literal, then maps each operand to a [`TplPiece`] and assembles variants with
/// [`assemble_concat_variants`]. See the module doc for the full rule.
fn resolve_concat_variants(
    arg: &Expr,
    consts: &HashMap<String, String>,
    cm: &SourceMap,
) -> Vec<String> {
    let Some(operands) = flatten_add_chain(arg) else {
        return Vec::new();
    };
    let has_literal_operand = operands
        .iter()
        .any(|o| matches!(unwrap_expr(o), Expr::Lit(Lit::Str(_))));
    if !has_literal_operand {
        return Vec::new();
    }
    let pieces: Vec<TplPiece> = operands
        .iter()
        .map(|o| concat_operand_piece(o, consts, cm))
        .collect();
    assemble_concat_variants(&pieces)
}

/// Flatten a left-associative `+` chain (`a + b + c` parses as `Bin{Bin{a,b},c}`) into its ordered
/// operands, or `None` if any operator in the chain is not `+` — never guessed for `-`, `??`, `||`, etc.
/// A wrapper (`(...)`, `as const`, ...) around the chain or one of its sub-chains is stripped via
/// [`unwrap_expr`] before matching.
fn flatten_add_chain(expr: &Expr) -> Option<Vec<&Expr>> {
    match unwrap_expr(expr) {
        Expr::Bin(b) => {
            if b.op != BinaryOp::Add {
                return None;
            }
            let mut operands = flatten_add_chain(&b.left)?;
            operands.push(&b.right);
            Some(operands)
        }
        other => Some(vec![other]),
    }
}

/// Map one `+`-chain operand to a [`TplPiece`]: a string literal is `Fixed`; an identifier/member that
/// resolves in `consts` (same lookup as `resolve_url_variants`'s own `Expr::Ident|Member` arm) is
/// `Fixed`; a ternary with BOTH arms string literals is the fan-out `Slot`; anything else (an unresolved
/// identifier, a call, a nested non-string expression) is the old `Fixed("{}")` placeholder.
fn concat_operand_piece(
    operand: &Expr,
    consts: &HashMap<String, String>,
    cm: &SourceMap,
) -> TplPiece {
    match unwrap_expr(operand) {
        Expr::Lit(Lit::Str(s)) => TplPiece::Fixed(s.value.as_str().unwrap_or_default().to_string()),
        e @ (Expr::Ident(_) | Expr::Member(_)) => match consts.get(&expr_text(e, cm)) {
            Some(v) => TplPiece::Fixed(v.clone()),
            None => TplPiece::Fixed("{}".to_string()),
        },
        Expr::Cond(c) => {
            if let (Expr::Lit(Lit::Str(cons)), Expr::Lit(Lit::Str(alt))) = (&*c.cons, &*c.alt) {
                TplPiece::Slot(
                    cons.value.as_str().unwrap_or_default().to_string(),
                    alt.value.as_str().unwrap_or_default().to_string(),
                )
            } else {
                TplPiece::Fixed("{}".to_string())
            }
        }
        _ => TplPiece::Fixed("{}".to_string()),
    }
}

/// Assemble a `+`-chain's pieces into URL variants: concatenate `Fixed` pieces inline, cartesian-product
/// `Slot` pieces, capped at 2 slots (a 3rd+ slot forces every slot in THIS chain back to fixed `"{}"`,
/// same bounded-output rule as [`resolve_template_variants`]), then dedup preserving first-seen order.
/// Standalone from `resolve_template_variants`'s assembly loop — deliberately NOT shared, so that loop's
/// existing tests stay byte-identical (see module doc / task notes): a concat chain has no quasis, so
/// pieces are just concatenated in sequence rather than interleaved with quasi text.
fn assemble_concat_variants(pieces: &[TplPiece]) -> Vec<String> {
    let slot_count = pieces
        .iter()
        .filter(|p| matches!(p, TplPiece::Slot(_, _)))
        .count();
    let mut variants = vec![String::new()];
    for p in pieces {
        match p {
            TplPiece::Fixed(s) => {
                for v in variants.iter_mut() {
                    v.push_str(s);
                }
            }
            TplPiece::Slot(cons, alt) => {
                if slot_count > 2 {
                    for v in variants.iter_mut() {
                        v.push_str("{}");
                    }
                } else {
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

/// One piece of a template literal between two quasis: either fixed text (the old `"{}"` placeholder),
/// or a conditional-literal fan-out slot carrying its two literal arm values.
enum TplPiece {
    Fixed(String),
    Slot(String, String),
}

/// Template literal -> URL variants. `/api/users/${id}` -> `["/api/users/{}"]`, same as before.
/// (`cond-literal-fanout-v1`): an interpolation whose expression is a ternary with BOTH arms string
/// literals is a fan-out slot instead of a fixed `{}` — e.g. `` `/users${isRegister ? '' : '/login'}` ``
/// -> `["/users", "/users/login"]`. Multiple slots cartesian-product together, capped at 2 slots (<=4
/// variants); a 3rd+ slot forces EVERY slot in this template back to the old fixed `{}` behavior, keeping
/// output bounded and deterministic. Variants are deduped preserving first-seen order.
fn resolve_template_variants(t: &Tpl) -> Vec<String> {
    let mut pieces: Vec<TplPiece> = Vec::with_capacity(t.exprs.len());
    let mut slot_count = 0usize;
    for e in t.exprs.iter() {
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
                    // Object literals and (below) string enums only — dotted, namespaced keys
                    // (`RouteKey.Asset`). A bare `const path = "/x"` is deliberately NOT captured: this map
                    // is project-wide and scope-insensitive (last-write-wins), so a bare common name
                    // (`path`, `url`, `base`) could shadow a same-named function parameter and mis-key an
                    // unrelated `axios.get(path)` — a guess dressed as a visible fact. `str-concat-url-v1`
                    // resolves the visible LITERAL operands of a concat, not bare-const-prefix indirection.
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
        // query-only URL, non-http scheme, whitespace — all stay unresolved with raw+method carried. The
        // first case, `` `${API_ROOT}${url}` ``, assembles to `"{}{}"` — a SECOND `{}` immediately after
        // the head, which is dynamic too (no literal path), so `consume_key_for`'s base-carrier head-drop
        // bucket ("{}"-head + "/"-headed remainder) does not fire here either; it stays unresolved.
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

    // --- base-carrier head-drop (`base-carrier-drop-v1`) ---

    #[test]
    fn base_carrier_head_drop_root_ping_keys_as_get_root() {
        // Boundary pin (accepted, not an accident): `` fetch(`${API}/`) `` assembles to `"{}/"` and
        // keys as `GET /` — the least-specific key. Same visible-fact contract as a literal
        // `fetch("/")` (which also keys `GET /`): the call demonstrably targets the base root.
        // The base-carries-a-path-prefix risk is the documented head-drop trade-off (near-miss
        // prefix class absorbs it); vetoing only the root form would be inconsistent with the
        // literal root-relative case.
        let out = extract_http_egress(&files(&[("a.tsx", "fetch(`${API}/`)")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /"));
    }

    #[test]
    fn base_carrier_head_drop_keys_the_visible_path_from_a_template() {
        // The liberation-shaped driving case: `` fetch(`${BASE_URL}/me/achievements`) `` assembles to
        // `"{}/me/achievements"` — one opaque leading `{}` (the base, invisible cross-file) followed by a
        // `/`-headed literal. The base is dropped, never valued; the visible literal keys the call.
        let out = extract_http_egress(&files(&[("a.tsx", "fetch(`${BASE_URL}/me/achievements`)")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /me/achievements"));
        assert!(out[0].raw.is_none());
    }

    #[test]
    fn base_carrier_head_drop_keeps_inner_interpolation_slots() {
        // A second `{}` NOT immediately after the head is an ordinary path-param slot, not a second
        // dynamic base — existing template-param semantics apply past the dropped head.
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get(`${base}/users/${id}`)")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /users/{}"));
    }

    #[test]
    fn base_carrier_head_drop_still_drops_the_query_suffix() {
        // Query-drop (`query-drop-v1`) composes with head-drop: the base is dropped AND the query is
        // dropped, leaving just the route path.
        let out = extract_http_egress(&files(&[("a.tsx", "fetch(`${B}/articles?limit=10`)")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /articles"));
    }

    #[test]
    fn base_carrier_head_drop_refuses_second_dynamic_head() {
        // `` `${a}${b}` `` assembles to `"{}{}"` — the piece right after the head is dynamic too (no
        // literal path at all), so this never-guesses rather than keying on nothing.
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get(`${a}${b}`)")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("`${a}${b}`"));
    }

    #[test]
    fn base_carrier_head_drop_refuses_non_slash_suffix() {
        // `` `${base}users` `` assembles to `"{}users"` — a non-`/`-headed literal suffix means the
        // segment boundary is invisible (could be a mid-segment concat like `base + "users"`), so it never
        // keys.
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get(`${base}users`)")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("`${base}users`"));
    }

    #[test]
    fn base_carrier_head_drop_refuses_protocol_relative_host() {
        // `` `${proto}//example.com/x` `` assembles to `"{}//example.com/x"` — the post-drop `//` means
        // the next piece is a HOST (protocol-relative URL), not a path, so it never keys as internal.
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get(`${proto}//example.com/x`)")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("`${proto}//example.com/x`"));
    }

    #[test]
    fn base_carrier_head_drop_does_not_disturb_literal_external_head() {
        // A literal `https://`-headed template is checked in the external branch BEFORE the head-drop
        // branch and is unaffected by it — mirrors the pinned expectation in
        // `absolute_url_becomes_a_host_carrying_key_for_the_external_bucket`.
        let out = extract_http_egress(&files(&[("a.tsx", "fetch(`https://api.ext.com/${p}`)")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET https://api.ext.com/{}"));
        assert!(out[0].raw.is_none());
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

    // --- generated-SDK receivers are not recognized (decision: generated SDKs are injection
    // adapters, not engine vocab — the former oazapfts-specific recognition lived here) ---

    #[test]
    fn former_qs_suffix_special_case_is_gone_trailing_interpolation_is_a_plain_placeholder() {
        // A trailing `${QS.query(...)}`-shaped interpolation used to be dropped entirely as
        // oazapfts-codegen's query-string suffix. That special case is gone: it now keys like any other
        // trailing interpolation, as an ordinary `{}` placeholder.
        let out = extract_http_egress(&files(&[(
            "activity.ts",
            r#"axios.get(`/activities${QS.query(QS.explode({ albumId }))}`);"#,
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /activities{}"));
    }

    // --- Angular HttpClient injection (`angular-httpclient-v1`) ---

    #[test]
    fn angular_constructor_param_property_http_client_is_recognized() {
        let out = extract_http_egress(&files(&[(
            "article.service.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "export class ArticleService {\n",
                "  constructor(private readonly http: HttpClient) {}\n",
                "  getArticles() {\n",
                "    this.http.get<{a: string}>('/articles');\n",
                "    this.http.post('/users', {});\n",
                "    this.http.delete(`/articles/${slug}`);\n",
                "  }\n",
                "}\n",
            ),
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("GET /articles".to_string()),
                Some("POST /users".to_string()),
                Some("DELETE /articles/{}".to_string()),
            ]
        );
    }

    #[test]
    fn angular_field_inject_http_client_is_recognized() {
        let out = extract_http_egress(&files(&[(
            "article.service.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "import { inject } from '@angular/core';\n",
                "export class ArticleService {\n",
                "  private http = inject(HttpClient);\n",
                "  getArticles() {\n",
                "    return this.http.get('/articles');\n",
                "  }\n",
                "}\n",
            ),
        )]));
        assert_eq!(keys(&out), vec![Some("GET /articles".to_string())]);
    }

    #[test]
    fn angular_local_const_inject_http_client_is_recognized() {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "import { inject } from '@angular/core';\n",
                "function useArticles() {\n",
                "  const http = inject(HttpClient);\n",
                "  return http.get('/x');\n",
                "}\n",
            ),
        )]));
        assert_eq!(keys(&out), vec![Some("GET /x".to_string())]);
    }

    #[test]
    fn angular_shape_without_the_import_is_not_recognized() {
        // Same shape, no `@angular/common/http` import anywhere in the file — never guessed.
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "export class ArticleService {\n  constructor(private readonly http) {}\n  x() { this.http.get('/x'); }\n}\n",
        )]));
        assert!(out.is_empty());
    }

    #[test]
    fn angular_gated_file_but_non_http_client_typed_receiver_is_not_recognized() {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "export class AuthService {\n",
                "  constructor(private readonly http: HttpClient, private readonly jwtService: JwtService) {}\n",
                "  x() {\n",
                "    this.jwtService.get('/x');\n",
                "  }\n",
                "}\n",
            ),
        )]));
        assert!(out.is_empty());
    }

    // --- `+` string-concatenation URLs (`str-concat-url-v1`) ---

    #[test]
    fn str_concat_literal_plus_variable_keys_as_param() {
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get('/profiles/' + username)")]));
        assert_eq!(out[0].key.as_deref(), Some("GET /profiles/{}"));
    }

    #[test]
    fn str_concat_three_way_with_trailing_literal() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.post('/profiles/' + username + '/follow', body)",
        )]));
        assert_eq!(out[0].key.as_deref(), Some("POST /profiles/{}/follow"));
    }

    #[test]
    fn str_concat_with_conditional_literal_fans_out() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.get('/articles' + (feed ? '/feed' : ''))",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("GET /articles/feed".to_string()),
                Some("GET /articles".to_string()),
            ]
        );
    }

    #[test]
    fn str_concat_with_no_string_literal_is_unresolved() {
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get(base + path)")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("base + path"));
    }

    #[test]
    fn str_concat_non_plus_operator_is_unresolved() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.get('/a' - x); axios.get('/b' ?? y);",
        )]));
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|c| c.key.is_none()));
    }

    #[test]
    fn str_concat_opaque_prefix_now_keys_via_head_drop() {
        // A bare `const BASE = '/api'; axios.get(BASE + '/users')` still never resolves the VALUE of
        // BASE: a bare undotted const is not captured by `const_map_fragment` (project-wide
        // scope-insensitive lookup would let a common name shadow a function param and mis-key —
        // never-guess). BASE -> `{}`, so the concat assembles to `"{}/users"`. That used to dead-end at
        // `base_relative_path`'s `{`-veto; now `consume_key_for`'s base-carrier head-drop bucket catches
        // this exact shape (single leading `{}` + `/`-headed literal) and drops the opaque base rather
        // than valuing it, keying on the visible `/users` literal. (str-concat-url-v1 resolves the visible
        // LITERAL operands, not const-prefix indirection — cross-layer-resolution.md; the shadow-risk
        // revert on BASE's value still stands.)
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "const BASE = '/api'; axios.get(BASE + '/users')",
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /users"));
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

    // --- request-body shape extraction (`body-shape-v1`) ---

    #[test]
    fn body_position_verbs_is_pinned_to_the_exact_set() {
        // T2 pin (rule-quality policy value inventory): a drift here (an added/removed verb) must fail
        // this test loudly rather than silently changing which call sites get body evidence.
        assert_eq!(BODY_POSITION_VERBS, &["POST", "PUT", "PATCH"]);
    }

    #[test]
    fn axios_post_nested_object_literal_witnesses_two_level_shape() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.post('/users', { user: { email, password } });",
        )]));
        assert_eq!(out.len(), 1);
        let body = out[0].body.as_ref().expect("body shape expected");
        assert_eq!(
            body.keys,
            vec!["user", "user.email", "user.password"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            body.complete_at,
            vec!["", "user"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn axios_put_with_identifier_body_is_none() {
        let out = extract_http_egress(&files(&[("a.tsx", "axios.put('/users/1', payload);")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].body.is_none());
    }

    #[test]
    fn axios_get_with_options_object_body_is_none() {
        // GET is not a body-position verb — `args[1]` here is a config object, not a body, regardless
        // of it being a static object literal.
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.get('/users', { params: { page: 1 } });",
        )]));
        assert_eq!(out.len(), 1);
        assert!(out[0].body.is_none());
    }

    #[test]
    fn missing_second_argument_is_never_a_witnessed_body() {
        // Absence is not evidence either way (an interceptor may inject a body invisibly).
        let out = extract_http_egress(&files(&[("a.tsx", "axios.post('/users');")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].body.is_none());
    }

    #[test]
    fn shorthand_body_prop_witnesses_the_key_without_descending() {
        let out = extract_http_egress(&files(&[("a.tsx", "axios.post('/users', { user });")]));
        let body = out[0].body.as_ref().unwrap();
        assert_eq!(body.keys, vec!["user".to_string()]);
        assert_eq!(body.complete_at, vec!["".to_string()]);
    }

    #[test]
    fn top_level_spread_marks_the_root_incomplete() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.post('/users', { ...defaults, user });",
        )]));
        let body = out[0].body.as_ref().unwrap();
        assert_eq!(body.keys, vec!["user".to_string()]);
        assert!(
            !body.complete_at.contains(&"".to_string()),
            "spread at the top level must suppress the root's completeness: {body:?}"
        );
    }

    #[test]
    fn empty_object_literal_body_is_a_witnessed_empty_shape() {
        // An explicit `{}` IS evidence (a witnessed empty body), unlike a missing `args[1]` — `Some`
        // with empty `keys` but `complete_at: [""]`.
        let out = extract_http_egress(&files(&[("a.tsx", "axios.post('/users', {});")]));
        let body = out[0].body.as_ref().unwrap();
        assert!(body.keys.is_empty());
        assert_eq!(body.complete_at, vec!["".to_string()]);
    }

    #[test]
    fn fetch_json_stringify_body_is_unwrapped() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            r#"fetch('/users', { method: 'POST', body: JSON.stringify({ name, email }) });"#,
        )]));
        let body = out[0].body.as_ref().unwrap();
        assert_eq!(body.keys, vec!["email".to_string(), "name".to_string()]);
        assert_eq!(body.complete_at, vec!["".to_string()]);
    }

    #[test]
    fn fetch_body_identifier_is_none() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "fetch('/users', { method: 'POST', body: someVar });",
        )]));
        assert!(out[0].body.is_none());
    }

    #[test]
    fn angular_http_client_post_body_is_witnessed() {
        let out = extract_http_egress(&files(&[(
            "article.service.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "export class ArticleService {\n",
                "  constructor(private readonly http: HttpClient) {}\n",
                "  create() {\n",
                "    this.http.post('/articles', { title });\n",
                "  }\n",
                "}\n",
            ),
        )]));
        let body = out[0].body.as_ref().unwrap();
        assert_eq!(body.keys, vec!["title".to_string()]);
        assert_eq!(body.complete_at, vec!["".to_string()]);
    }

    #[test]
    fn computed_member_mixed_verb_fanout_body_is_none() {
        // Mixed-verb fanout (`DELETE`/`POST`) never guesses which arm the object belongs to — `None`
        // for BOTH emitted consumes, even though a static object literal is visibly present.
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "axios[c ? 'delete' : 'post'](`/articles/${slug}/favorite`, { note });",
        )]));
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|c| c.body.is_none()));
    }

    #[test]
    fn depth_three_nesting_is_capped_at_depth_two() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.post('/x', { a: { b: { c: 1 } } });",
        )]));
        let body = out[0].body.as_ref().unwrap();
        assert_eq!(body.keys, vec!["a".to_string(), "a.b".to_string()]);
        assert_eq!(body.complete_at, vec!["".to_string(), "a".to_string()]);
    }

    // --- client provenance tag (`axios-defaults-base-v1`) ---

    fn clients(out: &[IoConsume]) -> Vec<Option<String>> {
        out.iter().map(|c| c.client.clone()).collect()
    }

    #[test]
    fn axios_member_call_is_tagged_axios() {
        let out = extract_http_egress(&files(&[("a.ts", r#"axios.get("/a");"#)]));
        assert_eq!(clients(&out), vec![Some("axios".to_string())]);
    }

    #[test]
    fn bare_axios_call_is_tagged_axios() {
        let out = extract_http_egress(&files(&[("a.ts", r#"axios("/a");"#)]));
        assert_eq!(clients(&out), vec![Some("axios".to_string())]);
    }

    #[test]
    fn axios_computed_member_call_is_tagged_axios() {
        let out = extract_http_egress(&files(&[("a.ts", "axios['post']('/a');")]));
        assert_eq!(clients(&out), vec![Some("axios".to_string())]);
    }

    #[test]
    fn axios_computed_member_fanout_is_tagged_axios_for_every_variant() {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "axios[favorited ? 'delete' : 'post'](`/articles/${slug}/favorite`);",
        )]));
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|c| c.client.as_deref() == Some("axios")));
    }

    #[test]
    fn ky_member_call_is_tagged_ky() {
        let out = extract_http_egress(&files(&[("a.ts", r#"ky.get("/a");"#)]));
        assert_eq!(clients(&out), vec![Some("ky".to_string())]);
    }

    #[test]
    fn bare_fetch_call_is_tagged_fetch() {
        let out = extract_http_egress(&files(&[("a.ts", r#"fetch("/a");"#)]));
        assert_eq!(clients(&out), vec![Some("fetch".to_string())]);
    }

    #[test]
    fn dollar_fetch_call_is_tagged_dollar_fetch() {
        let out = extract_http_egress(&files(&[("a.ts", r#"$fetch("/a");"#)]));
        assert_eq!(clients(&out), vec![Some("$fetch".to_string())]);
    }

    #[test]
    fn angular_http_client_call_is_tagged_angular() {
        let out = extract_http_egress(&files(&[(
            "article.service.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "export class ArticleService {\n",
                "  constructor(private readonly http: HttpClient) {}\n",
                "  getArticles() {\n",
                "    this.http.get('/articles');\n",
                "  }\n",
                "}\n",
            ),
        )]));
        assert_eq!(clients(&out), vec![Some("angular".to_string())]);
    }

    #[test]
    fn unresolved_consume_still_carries_its_client_tag() {
        // Dynamic URL (unresolved) — `client` is still set from the matcher, independent of whether the
        // URL itself resolved to a key.
        let out = extract_http_egress(&files(&[("a.ts", "axios.get(buildUrl(x));")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].client.as_deref(), Some("axios"));
    }
}
