//! Manual pathname-dispatch route provides: framework-less servers (raw Cloudflare Workers,
//! Node `http.createServer`, Deno/Bun serve) that route by comparing `url.pathname` against
//! string literals (`if (url.pathname === "/x")` chains, `switch (url.pathname)`), instead of a
//! decorator/router-registration vocabulary. swc-AST-based (mirrors `controller_decorators.rs`'s
//! per-file visitor shape), never-guess on every axis called out below.
//!
//! ## Per-function evidence gates (both required before any emission from that function)
//! 1. **Request context**: a param with TS type annotation `Request`, or named exactly `request`
//!    or `req` (covers untyped JS). Checked once per function signature; a function with neither
//!    contributes nothing at all (cheaper than gating each path test individually, and matches
//!    the false-positive corpus: `location.pathname`/`new URL(window.location.href)` sites live
//!    in client code that never takes a `request`/`req` param). Known residual FP of the
//!    name-based half: a service-worker fetch HELPER (`function onFetch(request) { const url =
//!    new URL(request.url); ... }`) passes both gates and emits its offline/cache routes as
//!    provides even though a service worker is not a server — accepted v1 tradeoff; revisit if
//!    a PWA corpus pulls (a `self.addEventListener` file-level veto is the likely fix).
//! 2. **URL provenance** for the pathname receiver actually compared: `<u>.pathname` where `<u>`
//!    is a `URL`-typed param or a local `const/let/var <u> = new URL(...)`, or a local alias
//!    (`const { pathname } = <u>`, incl. rename, or `const p = <u>.pathname`). A receiver that is
//!    itself a member-of-member (`request.nextUrl.pathname`) is deliberately NOT provenanced —
//!    only a bare-identifier receiver qualifies, which is what excludes Next middleware's
//!    `request.nextUrl.pathname` and any `router.pathname`/`location.pathname` shape.
//!
//! The real-world anchor: a dispatch function commonly receives `url: URL` as a typed parameter
//! injected by a cross-file wrapper rather than constructing it locally — gate 2 accepts a
//! `URL`-typed PARAM for exactly this reason, not just a same-function `new URL(...)`.
//!
//! ## Durable Object veto
//! An entire class body is skipped (no methods analyzed, DO or not) when the class has DO
//! evidence: a constructor param typed `DurableObjectState`, or an `implements`/`extends` clause
//! naming `DurableObject`. A Durable Object's `fetch()` routes are reachable only via
//! `stub.fetch` — an edge request 404s — so emitting them as `kind:"http"` provides would
//! over-claim public surface. The veto is types/`extends`-gated only: an untyped plain-JS DO
//! (`constructor(state, env)`, no clause) is undetectable without types and DOES emit its
//! internal routes — accepted v1 limit, documented rather than guessed around.
//!
//! ## Verb recognition
//! A verb mention is a binary comparison (`===`/`==`/`!==`/`!=`, either operand order) between a
//! string literal exactly matching `zzop_core::HTTP_KEY_VERBS` and either `<r>.method` (`<r>`
//! request-evidenced) or a local method alias (`const method = <r>.method` / `const { method } =
//! <r>`); or a `switch (<r>.method | alias)` case with such a literal. `!==` counts — same
//! mentioned-verb semantics as `next_pages_api.rs`'s `req.method !== "POST"` early-return.
//!
//! ## Path test
//! `===`/`==` (either operand order) between a pathname-provenanced receiver and a string literal
//! starting with `/` (a zero-interpolation template literal counts, cooked). Deliberately
//! excluded (v1 under-approximation): `!==`/`!=` path guards, `startsWith`/`includes`/regex,
//! interpolated templates, literals without a leading `/`, const indirection through an
//! unresolved identifier.
//!
//! ## Association algorithm
//! Per function body (independently — bindings/tests never leak across a nested function
//! boundary): every `IfStmt` reachable without crossing into a nested function is evaluated on
//! its own test, decomposed into `&&`-conjuncts (recursively unwrapping parens). Each conjunct is
//! either a path test, a verb test, or an `||` disjunction of same-shaped tests (an all-path `||`
//! contributes every disjunct's path; an all-verb `||` unions its verbs; a MIXED `||` — e.g.
//! `(path || flag)` — contributes nothing, never guessed). If the resulting path set is
//! non-empty: verbs come from the test's own conjuncts if any were found there, else from
//! recursively scanning the `if`'s consequent block for verb mentions (if-conditions,
//! switch-on-method) — stopping at nested function bodies and skipping the whole subtree of any
//! nested `IfStmt` whose OWN condition contains a path test (that nested `if` is a separate
//! route, evaluated independently; letting its verb scan leak into the parent would
//! cross-contaminate two different routes) — else `PATHNAME_DISPATCH_FALLBACK_VERBS`. One provide
//! is emitted per (path × verb); `line` is the path test's own line, `symbol` is the enclosing
//! function's name when nameable (`FnDecl` ident, class-method/object-method key, or `const name
//! = () => {}` binding name).
//!
//! A `SwitchStmt` whose discriminant is a pathname-provenanced receiver is handled the same way,
//! grouping consecutive empty-body cases onto the next non-empty body (fallthrough), scanning
//! that shared body for verb mentions (else fallback), with `line` = the case's own line.
//!
//! Exact-duplicate `(key, line, symbol)` triples are deduped; output order is deterministic
//! (occurrence order).
//!
//! ## Pre-gate deviation
//! The pre-gate checks for the bare substring `"pathname"`, not a literal `".pathname"`. The
//! canonical `const { pathname } = url; if (pathname === ...)` shape (module doc gate 2) never
//! spells a dot before `pathname` anywhere in the file — a literal `".pathname"` substring gate
//! would reject that required shape outright. `"pathname"` alone is still a cheap, useful
//! fast-path (a file that never mentions the word at all cannot match any recognized shape).

use std::collections::HashSet;

use swc_core::common::{BytePos, SourceMap};
use swc_core::ecma::ast::{
    ArrowExpr, BinaryOp, BlockStmt, BlockStmtOrExpr, Class, ClassDecl, ClassExpr, ClassMember,
    ClassMethod, Constructor, Expr, FnDecl, FnExpr, Function, GetterProp, IfStmt, Lit, MemberProp,
    MethodProp, ObjectPat, ObjectPatProp, ParamOrTsParamProp, Pat, PropName, SetterProp, Stmt,
    SwitchStmt, TsEntityName, TsParamPropParam, TsType, TsTypeAnn, VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_interface_key, IoProvide, HTTP_KEY_VERBS};

/// Verbs emitted for a pathname-guarded route whose block names no request-method comparison at
/// all. Same value (and same rationale) as the engine's `PAGES_API_FALLBACK_VERBS` for a
/// `pages/api` handler that names no method literal; the equality is sealed by a cross-crate pin
/// test (policy tier T2), not repeated here.
pub const PATHNAME_DISPATCH_FALLBACK_VERBS: [&str; 2] = ["GET", "POST"];

/// Extract `kind:"http"` route provides from manual pathname-dispatch sites in one file. See
/// module doc for the full recognizer spec. Returns an empty `Vec` (never panics) on an
/// unparseable file, same convention as every other swc-AST adapter in this crate.
pub fn extract_pathname_dispatch_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    // Cheap pre-gate: every recognized shape mentions "pathname" somewhere — either a direct
    // `.pathname` member access, or (deliberately widened from a literal ".pathname" substring
    // check — see module doc "Pre-gate deviation") a destructured/aliased bare `pathname`
    // identifier, which the canonical `const { pathname } = url` shape never spells with a dot.
    if !text.contains("pathname") {
        return Vec::new();
    }
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut collector = TopCollector {
        cm: cm_ref,
        rel,
        out: Vec::new(),
        pending_name: None,
    };
    module.visit_with(&mut collector);
    dedup_provides(collector.out)
}

fn dedup_provides(provides: Vec<IoProvide>) -> Vec<IoProvide> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(provides.len());
    for p in provides {
        let sig = (p.key.clone(), p.line, p.symbol.clone());
        if seen.insert(sig) {
            out.push(p);
        }
    }
    out
}

fn fallback_verbs() -> Vec<String> {
    PATHNAME_DISPATCH_FALLBACK_VERBS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn push_unique(list: &mut Vec<String>, v: String) {
    if !list.iter().any(|x| x == &v) {
        list.push(v);
    }
}

// ---------------------------------------------------------------------------------------------
// Module-level walk: finds every function-like node and every DO-vetoed class.
// ---------------------------------------------------------------------------------------------

struct TopCollector<'a> {
    cm: &'a SourceMap,
    rel: &'a str,
    out: Vec<IoProvide>,
    /// The binding name of an enclosing `const <name> = <arrow | anonymous function expr>`,
    /// consumed by the very next `visit_arrow_expr`/`visit_fn_expr` call (set right before
    /// recursing into a `VarDeclarator`'s children, cleared right after).
    pending_name: Option<String>,
}

impl TopCollector<'_> {
    fn handle_function(&mut self, function: &Function, symbol: Option<String>) {
        let Some(body) = &function.body else {
            return; // an overload signature / ambient declaration — no body to analyze
        };
        let (request_idents, url_provenanced) =
            build_fn_ctx_seed(function.params.iter().map(|p| &p.pat));
        self.run_body(body, symbol, request_idents, url_provenanced);
    }

    fn handle_arrow(&mut self, arrow: &ArrowExpr, symbol: Option<String>) {
        let BlockStmtOrExpr::BlockStmt(body) = &*arrow.body else {
            return; // expression-bodied arrow — no statements, so no `if`/`switch` to find
        };
        let (request_idents, url_provenanced) = build_fn_ctx_seed(arrow.params.iter());
        self.run_body(body, symbol, request_idents, url_provenanced);
    }

    fn run_body(
        &mut self,
        body: &BlockStmt,
        symbol: Option<String>,
        request_idents: HashSet<String>,
        url_provenanced: HashSet<String>,
    ) {
        // Gate 1: no request-evidenced param anywhere in this function's own signature —
        // never-guess, contribute nothing (module doc).
        if request_idents.is_empty() {
            return;
        }
        let mut bindings = BindingCollector {
            request_idents: request_idents.clone(),
            url_provenanced,
            pathname_aliases: HashSet::new(),
            method_aliases: HashSet::new(),
        };
        body.visit_with(&mut bindings);
        let ctx = FnCtx {
            symbol,
            request_idents,
            url_provenanced: bindings.url_provenanced,
            pathname_aliases: bindings.pathname_aliases,
            method_aliases: bindings.method_aliases,
        };
        let mut routes = RouteCollector {
            ctx: &ctx,
            cm: self.cm,
            rel: self.rel,
            out: &mut self.out,
        };
        body.visit_with(&mut routes);
    }
}

impl Visit for TopCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        if class_has_do_evidence(&n.class) {
            return; // DO veto — see module doc; skip the whole class body, DO or not
        }
        n.visit_children_with(self);
    }

    fn visit_class_expr(&mut self, n: &ClassExpr) {
        if class_has_do_evidence(&n.class) {
            return;
        }
        n.visit_children_with(self);
    }

    fn visit_fn_decl(&mut self, n: &FnDecl) {
        self.handle_function(&n.function, Some(n.ident.sym.to_string()));
        n.visit_children_with(self);
    }

    fn visit_fn_expr(&mut self, n: &FnExpr) {
        let symbol = n
            .ident
            .as_ref()
            .map(|i| i.sym.to_string())
            .or_else(|| self.pending_name.take());
        self.handle_function(&n.function, symbol);
        n.visit_children_with(self);
    }

    fn visit_arrow_expr(&mut self, n: &ArrowExpr) {
        let symbol = self.pending_name.take();
        self.handle_arrow(n, symbol);
        n.visit_children_with(self);
    }

    fn visit_class_method(&mut self, n: &ClassMethod) {
        let symbol = prop_name_string(&n.key);
        self.handle_function(&n.function, symbol);
        n.visit_children_with(self);
    }

    fn visit_method_prop(&mut self, n: &MethodProp) {
        let symbol = prop_name_string(&n.key);
        self.handle_function(&n.function, symbol);
        n.visit_children_with(self);
    }

    fn visit_var_declarator(&mut self, n: &VarDeclarator) {
        if let (Pat::Ident(bi), Some(init)) = (&n.name, &n.init) {
            if is_nameable_fn_value(init) {
                self.pending_name = Some(bi.id.sym.to_string());
            }
        }
        n.visit_children_with(self);
        self.pending_name = None;
    }
}

fn is_nameable_fn_value(expr: &Expr) -> bool {
    matches!(expr, Expr::Arrow(_)) || matches!(expr, Expr::Fn(f) if f.ident.is_none())
}

fn prop_name_string(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Durable Object veto
// ---------------------------------------------------------------------------------------------

fn class_has_do_evidence(class: &Class) -> bool {
    if let Some(super_class) = &class.super_class {
        if let Expr::Ident(id) = &**super_class {
            if id.sym == "DurableObject" {
                return true;
            }
        }
    }
    if class
        .implements
        .iter()
        .any(|clause| matches!(&*clause.expr, Expr::Ident(id) if id.sym == "DurableObject"))
    {
        return true;
    }
    class.body.iter().any(|member| match member {
        ClassMember::Constructor(ctor) => constructor_has_do_state_param(ctor),
        _ => false,
    })
}

fn constructor_has_do_state_param(ctor: &Constructor) -> bool {
    ctor.params.iter().any(|p| match p {
        ParamOrTsParamProp::Param(param) => is_do_state_pat(&param.pat),
        ParamOrTsParamProp::TsParamProp(tpp) => match &tpp.param {
            TsParamPropParam::Ident(bi) => {
                type_ann_is(bi.type_ann.as_deref(), "DurableObjectState")
            }
            TsParamPropParam::Assign(_) => false,
        },
    })
}

fn is_do_state_pat(pat: &Pat) -> bool {
    matches!(pat, Pat::Ident(bi) if type_ann_is(bi.type_ann.as_deref(), "DurableObjectState"))
}

/// A bare `: Name` type annotation — a single-identifier `TsTypeRef` named exactly `name`.
fn type_ann_is(ann: Option<&TsTypeAnn>, name: &str) -> bool {
    let Some(ann) = ann else { return false };
    matches!(&*ann.type_ann, TsType::TsTypeRef(tr) if matches!(&tr.type_name, TsEntityName::Ident(id) if id.sym == name))
}

// ---------------------------------------------------------------------------------------------
// Per-function context: request/URL/pathname/method provenance
// ---------------------------------------------------------------------------------------------

struct FnCtx {
    symbol: Option<String>,
    request_idents: HashSet<String>,
    url_provenanced: HashSet<String>,
    pathname_aliases: HashSet<String>,
    method_aliases: HashSet<String>,
}

/// Seeds gate 1 (`request_idents`) and part of gate 2 (`url_provenanced`) from a function's own
/// parameter list — see module doc gates 1/2.
fn build_fn_ctx_seed<'p>(
    pats: impl Iterator<Item = &'p Pat>,
) -> (HashSet<String>, HashSet<String>) {
    let mut request_idents = HashSet::new();
    let mut url_provenanced = HashSet::new();
    for pat in pats {
        if let Pat::Ident(bi) = pat {
            let name = bi.id.sym.to_string();
            let is_request = name == "request"
                || name == "req"
                || type_ann_is(bi.type_ann.as_deref(), "Request");
            let is_url = type_ann_is(bi.type_ann.as_deref(), "URL");
            if is_request {
                request_idents.insert(name.clone());
            }
            if is_url {
                url_provenanced.insert(name);
            }
        }
    }
    (request_idents, url_provenanced)
}

/// Collects local bindings that extend gate 2 (URL provenance) and the method-alias vocabulary,
/// scanning a function body WITHOUT crossing into any nested function's own scope (module doc:
/// "never let bindings ... leak across a nested function boundary").
struct BindingCollector {
    request_idents: HashSet<String>,
    url_provenanced: HashSet<String>,
    pathname_aliases: HashSet<String>,
    method_aliases: HashSet<String>,
}

impl Visit for BindingCollector {
    fn visit_fn_decl(&mut self, _: &FnDecl) {}
    fn visit_fn_expr(&mut self, _: &FnExpr) {}
    fn visit_arrow_expr(&mut self, _: &ArrowExpr) {}
    fn visit_class_method(&mut self, _: &ClassMethod) {}
    fn visit_method_prop(&mut self, _: &MethodProp) {}
    fn visit_getter_prop(&mut self, _: &GetterProp) {}
    fn visit_setter_prop(&mut self, _: &SetterProp) {}

    fn visit_var_declarator(&mut self, n: &VarDeclarator) {
        if let Some(init) = &n.init {
            match &n.name {
                Pat::Ident(bi) => {
                    let name = bi.id.sym.to_string();
                    if is_new_url_call(init) {
                        self.url_provenanced.insert(name);
                    } else if is_pathname_member(init, &self.url_provenanced) {
                        self.pathname_aliases.insert(name);
                    } else if is_method_member(init, &self.request_idents) {
                        self.method_aliases.insert(name);
                    }
                }
                Pat::Object(op) => {
                    if let Expr::Ident(id) = &**init {
                        let src = id.sym.to_string();
                        if self.url_provenanced.contains(&src) {
                            for (key, local) in object_pat_bindings(op) {
                                if key == "pathname" {
                                    self.pathname_aliases.insert(local);
                                }
                            }
                        }
                        if self.request_idents.contains(&src) {
                            for (key, local) in object_pat_bindings(op) {
                                if key == "method" {
                                    self.method_aliases.insert(local);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        n.visit_children_with(self);
    }
}

fn is_new_url_call(expr: &Expr) -> bool {
    matches!(expr, Expr::New(n) if matches!(&*n.callee, Expr::Ident(id) if id.sym == "URL"))
}

/// `<u>.pathname` where `<u>` is a bare identifier in `url_provenanced` — a member-of-member
/// receiver (`request.nextUrl.pathname`) never matches since `m.obj` must itself be `Expr::Ident`.
fn is_pathname_member(expr: &Expr, url_provenanced: &HashSet<String>) -> bool {
    let Expr::Member(m) = expr else { return false };
    let Expr::Ident(obj) = &*m.obj else {
        return false;
    };
    if !url_provenanced.contains(obj.sym.as_str()) {
        return false;
    }
    matches!(&m.prop, MemberProp::Ident(p) if p.sym == "pathname")
}

/// `<r>.method` where `<r>` is a bare identifier in `request_idents`.
fn is_method_member(expr: &Expr, request_idents: &HashSet<String>) -> bool {
    let Expr::Member(m) = expr else { return false };
    let Expr::Ident(obj) = &*m.obj else {
        return false;
    };
    if !request_idents.contains(obj.sym.as_str()) {
        return false;
    }
    matches!(&m.prop, MemberProp::Ident(p) if p.sym == "method")
}

/// `(source key, local bound name)` pairs from an object pattern: shorthand `{ pathname }` binds
/// under its own name, `{ pathname: p }` renames.
fn object_pat_bindings(op: &ObjectPat) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for prop in &op.props {
        match prop {
            ObjectPatProp::Assign(a) => {
                let name = a.key.id.sym.to_string();
                out.push((name.clone(), name));
            }
            ObjectPatProp::KeyValue(kv) => {
                let source = match &kv.key {
                    PropName::Ident(i) => i.sym.to_string(),
                    PropName::Str(s) => s.value.as_str().unwrap_or_default().to_string(),
                    _ => continue,
                };
                if let Pat::Ident(bi) = &*kv.value {
                    out.push((source, bi.id.sym.to_string()));
                }
            }
            ObjectPatProp::Rest(_) => {}
        }
    }
    out
}

fn is_pathname_receiver(expr: &Expr, ctx: &FnCtx) -> bool {
    match expr {
        Expr::Ident(id) => ctx.pathname_aliases.contains(id.sym.as_str()),
        Expr::Member(_) => is_pathname_member(expr, &ctx.url_provenanced),
        _ => false,
    }
}

fn is_method_receiver(expr: &Expr, ctx: &FnCtx) -> bool {
    match expr {
        Expr::Ident(id) => ctx.method_aliases.contains(id.sym.as_str()),
        Expr::Member(_) => is_method_member(expr, &ctx.request_idents),
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------
// Test/conjunct classification
// ---------------------------------------------------------------------------------------------

fn unwrap_parens(mut expr: &Expr) -> &Expr {
    while let Expr::Paren(p) = expr {
        expr = &p.expr;
    }
    expr
}

fn split_and(expr: &Expr) -> Vec<&Expr> {
    let e = unwrap_parens(expr);
    if let Expr::Bin(b) = e {
        if b.op == BinaryOp::LogicalAnd {
            let mut out = split_and(&b.left);
            out.extend(split_and(&b.right));
            return out;
        }
    }
    vec![e]
}

fn split_or(expr: &Expr) -> Vec<&Expr> {
    let e = unwrap_parens(expr);
    if let Expr::Bin(b) = e {
        if b.op == BinaryOp::LogicalOr {
            let mut out = split_or(&b.left);
            out.extend(split_or(&b.right));
            return out;
        }
    }
    vec![e]
}

/// A path-test literal: a plain string literal, or a zero-interpolation template literal
/// (cooked) — see module doc's "Path test".
fn path_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        Expr::Tpl(t) if t.exprs.is_empty() && t.quasis.len() == 1 => t.quasis[0]
            .cooked
            .as_ref()
            .and_then(|c| c.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// A verb-test literal: a plain string literal only (no template-literal vocabulary for verbs).
fn verb_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}

/// A single path test: `===`/`==` only (never `!==`/`!=` — module doc), either operand order,
/// between a pathname-provenanced receiver and a `/`-leading literal. Returns the literal path
/// and the comparison's own span start (used for `IoProvide::line`).
fn path_test(expr: &Expr, ctx: &FnCtx) -> Option<(String, BytePos)> {
    let Expr::Bin(b) = unwrap_parens(expr) else {
        return None;
    };
    if !matches!(b.op, BinaryOp::EqEq | BinaryOp::EqEqEq) {
        return None;
    }
    let lit = if is_pathname_receiver(&b.left, ctx) {
        &b.right
    } else if is_pathname_receiver(&b.right, ctx) {
        &b.left
    } else {
        return None;
    };
    let path = path_literal(lit)?;
    if !path.starts_with('/') {
        return None;
    }
    Some((path, b.span.lo))
}

/// A single verb mention: `===`/`==`/`!==`/`!=` (mentioned-verb semantics — module doc), either
/// operand order, between a method-provenanced receiver and an `HTTP_KEY_VERBS` literal.
fn verb_test(expr: &Expr, ctx: &FnCtx) -> Option<String> {
    let Expr::Bin(b) = unwrap_parens(expr) else {
        return None;
    };
    if !matches!(
        b.op,
        BinaryOp::EqEq | BinaryOp::EqEqEq | BinaryOp::NotEq | BinaryOp::NotEqEq
    ) {
        return None;
    }
    let lit = if is_method_receiver(&b.left, ctx) {
        &b.right
    } else if is_method_receiver(&b.right, ctx) {
        &b.left
    } else {
        return None;
    };
    let verb = verb_literal(lit)?;
    if HTTP_KEY_VERBS.contains(&verb.as_str()) {
        Some(verb)
    } else {
        None
    }
}

/// What one `&&`-conjunct of an `if`'s test contributes.
enum Conjunct {
    Paths(Vec<(String, BytePos)>),
    Verbs(Vec<String>),
    /// Not a path test, verb test, or all-same-shape `||` of either — contributes nothing
    /// (covers a mixed `(path || flag)` disjunction — module doc: never guessed).
    Other,
}

fn classify_conjunct(expr: &Expr, ctx: &FnCtx) -> Conjunct {
    let e = unwrap_parens(expr);
    if let Expr::Bin(b) = e {
        if b.op == BinaryOp::LogicalOr {
            return classify_or_disjuncts(&split_or(e), ctx);
        }
    }
    if let Some(p) = path_test(e, ctx) {
        return Conjunct::Paths(vec![p]);
    }
    if let Some(v) = verb_test(e, ctx) {
        return Conjunct::Verbs(vec![v]);
    }
    Conjunct::Other
}

fn classify_or_disjuncts(disjuncts: &[&Expr], ctx: &FnCtx) -> Conjunct {
    let mut paths = Vec::new();
    let mut all_path = true;
    for d in disjuncts {
        match path_test(d, ctx) {
            Some(p) => paths.push(p),
            None => {
                all_path = false;
                break;
            }
        }
    }
    if all_path {
        return Conjunct::Paths(paths);
    }
    let mut verbs = Vec::new();
    let mut all_verb = true;
    for d in disjuncts {
        match verb_test(d, ctx) {
            Some(v) => push_unique(&mut verbs, v),
            None => {
                all_verb = false;
                break;
            }
        }
    }
    if all_verb {
        return Conjunct::Verbs(verbs);
    }
    Conjunct::Other
}

// ---------------------------------------------------------------------------------------------
// Association: IfStmt / SwitchStmt -> IoProvide
// ---------------------------------------------------------------------------------------------

/// Walks a whole function body (module doc "Association algorithm"), evaluating every `IfStmt`
/// and pathname-keyed `SwitchStmt` reachable without crossing a nested function boundary.
struct RouteCollector<'a> {
    ctx: &'a FnCtx,
    cm: &'a SourceMap,
    rel: &'a str,
    out: &'a mut Vec<IoProvide>,
}

impl Visit for RouteCollector<'_> {
    fn visit_fn_decl(&mut self, _: &FnDecl) {}
    fn visit_fn_expr(&mut self, _: &FnExpr) {}
    fn visit_arrow_expr(&mut self, _: &ArrowExpr) {}
    fn visit_class_method(&mut self, _: &ClassMethod) {}
    fn visit_method_prop(&mut self, _: &MethodProp) {}
    fn visit_getter_prop(&mut self, _: &GetterProp) {}
    fn visit_setter_prop(&mut self, _: &SetterProp) {}

    fn visit_if_stmt(&mut self, n: &IfStmt) {
        process_if(n, self.ctx, self.cm, self.rel, self.out);
        n.visit_children_with(self);
    }

    fn visit_switch_stmt(&mut self, n: &SwitchStmt) {
        process_switch(n, self.ctx, self.cm, self.rel, self.out);
        n.visit_children_with(self);
    }
}

fn process_if(n: &IfStmt, ctx: &FnCtx, cm: &SourceMap, rel: &str, out: &mut Vec<IoProvide>) {
    let conjuncts = split_and(&n.test);
    let mut paths: Vec<(String, BytePos)> = Vec::new();
    let mut verbs: Vec<String> = Vec::new();
    for c in &conjuncts {
        match classify_conjunct(c, ctx) {
            Conjunct::Paths(p) => paths.extend(p),
            Conjunct::Verbs(vs) => {
                for v in vs {
                    push_unique(&mut verbs, v);
                }
            }
            Conjunct::Other => {}
        }
    }
    if paths.is_empty() {
        return;
    }
    let final_verbs = if !verbs.is_empty() {
        verbs
    } else {
        let scanned = scan_verb_mentions(&n.cons, ctx);
        if !scanned.is_empty() {
            scanned
        } else {
            fallback_verbs()
        }
    };
    for (path, pos) in &paths {
        let line = crate::line_of(cm, *pos);
        emit_routes(rel, path, line, ctx.symbol.clone(), &final_verbs, out);
    }
}

fn process_switch(
    sw: &SwitchStmt,
    ctx: &FnCtx,
    cm: &SourceMap,
    rel: &str,
    out: &mut Vec<IoProvide>,
) {
    if !is_pathname_receiver(&sw.discriminant, ctx) {
        return;
    }
    let mut i = 0;
    while i < sw.cases.len() {
        // Group consecutive empty-body cases with the next non-empty body (fallthrough).
        let mut end = i;
        while sw.cases[end].cons.is_empty() && end + 1 < sw.cases.len() {
            end += 1;
        }
        let mut verbs = Vec::new();
        scan_block_for_verbs(&sw.cases[end].cons, ctx, &mut verbs);
        let verbs = if verbs.is_empty() {
            fallback_verbs()
        } else {
            verbs
        };
        for case in &sw.cases[i..=end] {
            if let Some(test) = &case.test {
                if let Some(path) = path_literal(test) {
                    if path.starts_with('/') {
                        let line = crate::line_of(cm, case.span.lo);
                        emit_routes(rel, &path, line, ctx.symbol.clone(), &verbs, out);
                    }
                }
            }
            // A `default:` case (no test) contributes no path.
        }
        i = end + 1;
    }
}

fn emit_routes(
    rel: &str,
    path: &str,
    line: u32,
    symbol: Option<String>,
    verbs: &[String],
    out: &mut Vec<IoProvide>,
) {
    for verb in verbs {
        out.push(IoProvide {
            kind: "http".to_string(),
            key: http_interface_key(verb, path),
            file: rel.to_string(),
            line,
            symbol: symbol.clone(),
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Fallback verb-mention scan (module doc: "recursively scanning the if's consequent block")
// ---------------------------------------------------------------------------------------------

fn scan_verb_mentions(stmt: &Stmt, ctx: &FnCtx) -> Vec<String> {
    let mut out = Vec::new();
    scan_stmt_for_verbs(stmt, ctx, &mut out);
    out
}

fn scan_block_for_verbs(stmts: &[Stmt], ctx: &FnCtx, out: &mut Vec<String>) {
    for s in stmts {
        scan_stmt_for_verbs(s, ctx, out);
    }
}

fn scan_stmt_for_verbs(stmt: &Stmt, ctx: &FnCtx, out: &mut Vec<String>) {
    match stmt {
        Stmt::Block(b) => scan_block_for_verbs(&b.stmts, ctx, out),
        Stmt::If(i) => {
            let conjuncts = split_and(&i.test);
            let classified: Vec<Conjunct> = conjuncts
                .iter()
                .map(|c| classify_conjunct(c, ctx))
                .collect();
            if classified.iter().any(|c| matches!(c, Conjunct::Paths(_))) {
                // A separate route lives here (module doc: skip the whole subtree so its verbs
                // never leak into this scan).
                return;
            }
            for c in classified {
                if let Conjunct::Verbs(vs) = c {
                    for v in vs {
                        push_unique(out, v);
                    }
                }
            }
            scan_stmt_for_verbs(&i.cons, ctx, out);
            if let Some(alt) = &i.alt {
                scan_stmt_for_verbs(alt, ctx, out);
            }
        }
        Stmt::Switch(sw) if is_method_receiver(&sw.discriminant, ctx) => {
            for case in &sw.cases {
                let Some(test) = &case.test else { continue };
                let Some(v) = verb_literal(test) else {
                    continue;
                };
                if HTTP_KEY_VERBS.contains(&v.as_str()) {
                    push_unique(out, v);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    //! Coverage for `extract_pathname_dispatch_provides`: the canonical corpus shapes (A-D), the
    //! switch fallthrough-grouping shape, verb-mention edge cases, every never-guess FP guard, the
    //! Durable Object veto, and the pre-gate.
    use super::*;

    fn keys(out: &[IoProvide]) -> Vec<String> {
        out.iter().map(|p| p.key.clone()).collect()
    }

    // -- Shape A: nested method ifs, destructured pathname, method alias, `url: URL` param --

    #[test]
    fn shape_a_nested_method_ifs_with_destructured_pathname() {
        let src = concat!(
            "async function dispatch(request: Request, env: Env, url: URL): Promise<Response> {\n",
            "  const { pathname } = url;\n",
            "  const method = request.method;\n",
            "  if (pathname === \"/me/achievements\") {\n",
            "    const playerId = extractPlayerId(request);\n",
            "    if (!playerId) return jsonErr(401, \"unauthorized\", \"x\");\n",
            "    if (method === \"GET\") return handleGet(playerId, env);\n",
            "    if (method === \"POST\") return handlePost(playerId, request, env);\n",
            "  }\n",
            "  return jsonErr(404, \"not_found\", \"Route not found\");\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("worker.ts", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(got, vec!["GET /me/achievements", "POST /me/achievements"]);
        assert!(out.iter().all(|p| p.symbol.as_deref() == Some("dispatch")));
        assert!(out.iter().all(|p| p.file == "worker.ts"));
        let path_test_line = 4; // `if (pathname === "/me/achievements") {`
        assert!(out.iter().all(|p| p.line == path_test_line));
    }

    // -- Shape B: method-first with an OR path group (health-check idiom) --

    #[test]
    fn shape_b_method_first_with_or_path_group() {
        let src = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  if (request.method === \"GET\" && (url.pathname === \"/\" || url.pathname === \"/health\")) {\n",
            "    return ok();\n",
            "  }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("worker.ts", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(got, vec!["GET /", "GET /health"]);
    }

    // -- Shape C: compound guard in a plain function --

    #[test]
    fn shape_c_compound_guard_plain_function() {
        let src = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  if (url.pathname === \"/apply\" && request.method === \"POST\") {\n",
            "    return ok();\n",
            "  }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("worker.ts", src);
        assert_eq!(keys(&out), vec!["POST /apply"]);
    }

    // -- Shape D: object-literal Workers entry, untyped JS, fallback verbs --

    #[test]
    fn shape_d_object_literal_workers_entry_fallback_verbs() {
        let src = concat!(
            "export default {\n",
            "  async fetch(request, env) {\n",
            "    const url = new URL(request.url);\n",
            "    if (url.pathname === \"/webhook\") {\n",
            "      return handle(request, env);\n",
            "    }\n",
            "  },\n",
            "};\n"
        );
        let out = extract_pathname_dispatch_provides("worker.js", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(got, vec!["GET /webhook", "POST /webhook"]);
        assert!(out.iter().all(|p| p.symbol.as_deref() == Some("fetch")));
    }

    // -- switch (url.pathname): method-if case, no-method case, fallthrough-grouped DELETE case --

    #[test]
    fn switch_on_pathname_with_fallthrough_grouping() {
        let src = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  switch (url.pathname) {\n",
            "    case \"/a\":\n",
            "      if (request.method === \"GET\") return ok();\n",
            "      break;\n",
            "    case \"/b\":\n",
            "      doSomething();\n",
            "      break;\n",
            "    case \"/d\":\n",
            "    case \"/e\":\n",
            "      if (request.method === \"DELETE\") return ok();\n",
            "      break;\n",
            "  }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("worker.ts", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(
            got,
            vec!["DELETE /d", "DELETE /e", "GET /a", "GET /b", "POST /b"]
        );
    }

    // -- Verb `!==` mention --

    #[test]
    fn verb_not_equal_mention_counts() {
        let src = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  if (url.pathname === \"/x\") {\n",
            "    if (request.method !== \"POST\") return err();\n",
            "    return ok();\n",
            "  }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("worker.ts", src);
        assert_eq!(keys(&out), vec!["POST /x"]);
    }

    // -- Reversed operands, both path and verb --

    #[test]
    fn reversed_operands_both_path_and_verb() {
        let src = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  if (\"/x\" === url.pathname && \"POST\" === request.method) {\n",
            "    return ok();\n",
            "  }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("worker.ts", src);
        assert_eq!(keys(&out), vec!["POST /x"]);
    }

    // -- Verb-only OR disjunction unions its verbs --

    #[test]
    fn verb_only_or_disjunction_unions_verbs() {
        let src = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  if (url.pathname === \"/x\" && (request.method === \"PUT\" || request.method === \"DELETE\")) {\n",
            "    return ok();\n",
            "  }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("worker.ts", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(got, vec!["DELETE /x", "PUT /x"]);
    }

    // -- Mixed OR (path || flag) is discarded entirely --

    #[test]
    fn mixed_or_path_and_flag_is_discarded() {
        let src = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  if (url.pathname === \"/a\" || someFlag) {\n",
            "    return ok();\n",
            "  }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("worker.ts", src);
        assert!(out.is_empty(), "{out:?}");
    }

    // -- FP guard: `location.pathname` with no request evidence --

    #[test]
    fn fp_guard_location_pathname_no_request_evidence() {
        let src = concat!(
            "function onClick() {\n",
            "  if (location.pathname === \"/about\") {\n",
            "    return ok();\n",
            "  }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("client.ts", src);
        assert!(out.is_empty(), "{out:?}");
    }

    // -- FP guard: `new URL(window.location.href)` in a no-request function --

    #[test]
    fn fp_guard_new_url_from_window_location_no_request_evidence() {
        let src = concat!(
            "function route() {\n",
            "  const u = new URL(window.location.href);\n",
            "  if (u.pathname === \"/x\") {\n",
            "    return ok();\n",
            "  }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("client.ts", src);
        assert!(out.is_empty(), "{out:?}");
    }

    // -- FP guard: `request.nextUrl.pathname` (member-of-member receiver, not provenanced) --

    #[test]
    fn fp_guard_next_middleware_nexturl_pathname() {
        let src = concat!(
            "function middleware(request: Request) {\n",
            "  if (request.nextUrl.pathname === \"/x\") {\n",
            "    return ok();\n",
            "  }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("middleware.ts", src);
        assert!(out.is_empty(), "{out:?}");
    }

    // -- DO veto via constructor param typed DurableObjectState --

    #[test]
    fn do_veto_via_constructor_state_param() {
        let src = concat!(
            "class Room {\n",
            "  constructor(state: DurableObjectState, env: unknown) {}\n",
            "  async fetch(request: Request, url: URL) {\n",
            "    if (url.pathname === \"/apply\" && request.method === \"POST\") {\n",
            "      return ok();\n",
            "    }\n",
            "  }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("room.ts", src);
        assert!(out.is_empty(), "{out:?}");
    }

    // -- DO veto via `implements DurableObject` / `extends DurableObject` --

    #[test]
    fn do_veto_via_implements_and_extends_durable_object() {
        let implements_src = concat!(
            "class Room implements DurableObject {\n",
            "  async fetch(request: Request, url: URL) {\n",
            "    if (url.pathname === \"/apply\" && request.method === \"POST\") {\n",
            "      return ok();\n",
            "    }\n",
            "  }\n",
            "}\n"
        );
        assert!(extract_pathname_dispatch_provides("room.ts", implements_src).is_empty());

        let extends_src = concat!(
            "class Room extends DurableObject {\n",
            "  async fetch(request: Request, url: URL) {\n",
            "    if (url.pathname === \"/apply\" && request.method === \"POST\") {\n",
            "      return ok();\n",
            "    }\n",
            "  }\n",
            "}\n"
        );
        assert!(extract_pathname_dispatch_provides("room.ts", extends_src).is_empty());
    }

    // -- Exclusions: startsWith, interpolated template, no leading slash, `!==` path guard --

    #[test]
    fn exclusions_never_emit() {
        let starts_with = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  if (url.pathname.startsWith(\"/api\")) { return ok(); }\n",
            "}\n"
        );
        assert!(extract_pathname_dispatch_provides("w.ts", starts_with).is_empty());

        let interpolated = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  const id = \"1\";\n",
            "  if (url.pathname === `/x/${id}`) { return ok(); }\n",
            "}\n"
        );
        assert!(extract_pathname_dispatch_provides("w.ts", interpolated).is_empty());

        let no_leading_slash = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  if (url.pathname === \"x\") { return ok(); }\n",
            "}\n"
        );
        assert!(extract_pathname_dispatch_provides("w.ts", no_leading_slash).is_empty());

        let not_equal_guard = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  if (url.pathname !== \"/x\") { return err(); }\n",
            "  return ok();\n",
            "}\n"
        );
        assert!(extract_pathname_dispatch_provides("w.ts", not_equal_guard).is_empty());
    }

    // -- Zero-interpolation template literal path counts --

    #[test]
    fn zero_interpolation_template_literal_path_is_emitted() {
        let src = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  if (url.pathname === `/t`) { return ok(); }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("w.ts", src);
        assert_eq!(keys(&out), vec!["GET /t", "POST /t"]);
    }

    // -- Key normalization sanity: double slashes / trailing slash collapse via http_interface_key --

    #[test]
    fn key_normalization_sanity_via_http_interface_key() {
        let src = concat!(
            "function dispatch(request: Request, url: URL) {\n",
            "  if (url.pathname === \"/api//x/\" && request.method === \"GET\") { return ok(); }\n",
            "}\n"
        );
        let out = extract_pathname_dispatch_provides("w.ts", src);
        assert_eq!(keys(&out), vec!["GET /api/x"]);
    }

    // -- Pre-gate: no `.pathname` anywhere in the file --

    #[test]
    fn pre_gate_no_pathname_in_file_yields_empty() {
        assert!(extract_pathname_dispatch_provides("w.ts", "export const x = 1;\n").is_empty());
        let plain_server = concat!(
            "function dispatch(request: Request) {\n",
            "  return new Response(\"ok\");\n",
            "}\n"
        );
        assert!(extract_pathname_dispatch_provides("w.ts", plain_server).is_empty());
    }
}
