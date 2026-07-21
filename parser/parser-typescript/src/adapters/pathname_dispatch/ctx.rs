// Per-function context: request/URL/pathname/method provenance

use std::collections::{HashMap, HashSet};

use swc_core::common::BytePos;
use swc_core::ecma::ast::{
    ArrowExpr, Callee, ClassMethod, Expr, FnDecl, FnExpr, GetterProp, Lit, MemberProp, MethodProp,
    ObjectPat, ObjectPatProp, Pat, PropName, SetterProp, TsEntityName, TsType, TsTypeAnn,
    VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use super::regex_path::regex_to_route_path;

/// A bare `: Name` type annotation — a single-identifier `TsTypeRef` named exactly `name`.
pub(super) fn type_ann_is(ann: Option<&TsTypeAnn>, name: &str) -> bool {
    let Some(ann) = ann else { return false };
    matches!(&*ann.type_ann, TsType::TsTypeRef(tr) if matches!(&tr.type_name, TsEntityName::Ident(id) if id.sym == name))
}

pub(super) struct FnCtx {
    pub(super) symbol: Option<String>,
    pub(super) request_idents: HashSet<String>,
    pub(super) url_provenanced: HashSet<String>,
    pub(super) pathname_aliases: HashSet<String>,
    pub(super) method_aliases: HashSet<String>,
    /// `const m = pathname.match(/re/)` bindings whose regex converted to a route path: the local
    /// name -> (route path, the regex literal's span for `IoProvide::line`). A later `if (m && ...)`
    /// conjunct referencing `m` is a path test (see `classify::match_path_test`).
    pub(super) pathname_match_routes: HashMap<String, (String, BytePos)>,
}

/// Seeds gate 1 (`request_idents`) and part of gate 2 (`url_provenanced`) from a function's own
/// parameter list — see module doc gates 1/2.
pub(super) fn build_fn_ctx_seed<'p>(
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
pub(super) struct BindingCollector {
    pub(super) request_idents: HashSet<String>,
    pub(super) url_provenanced: HashSet<String>,
    pub(super) pathname_aliases: HashSet<String>,
    pub(super) method_aliases: HashSet<String>,
    pub(super) pathname_match_routes: HashMap<String, (String, BytePos)>,
    /// Names bound to a `pathname.match` MORE THAN ONCE in this flat (block-scope-unaware) walk —
    /// ambiguous, so removed from `pathname_match_routes` and never re-recorded (never-guess).
    pub(super) pathname_match_poisoned: HashSet<String>,
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
                    } else if let Some(route) =
                        pathname_match_route(init, &self.pathname_aliases, &self.url_provenanced)
                    {
                        // Name-keyed over a flat, block-scope-unaware walk: a name bound to a
                        // pathname.match MORE THAN ONCE (sibling block scopes reusing `const m`)
                        // is ambiguous. Never-guess — poison it so neither `if (m && ...)`
                        // resolves, rather than mis-attribute the last regex's route to an earlier
                        // block's guard.
                        if self.pathname_match_poisoned.contains(&name)
                            || self.pathname_match_routes.remove(&name).is_some()
                        {
                            self.pathname_match_poisoned.insert(name);
                        } else {
                            self.pathname_match_routes.insert(name, route);
                        }
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

/// `<pathname>.match(/re/)` where `<pathname>` is pathname-provenanced and the sole argument is a
/// regex literal that `regex_to_route_path` converts -> `(route path, regex-literal span)`. `None`
/// for a wrong receiver/method, a non-literal or multi/spread argument, or an unconvertible regex
/// (never-guess — the binding is simply not recorded, so no route is emitted for it).
pub(super) fn pathname_match_route(
    expr: &Expr,
    pathname_aliases: &HashSet<String>,
    url_provenanced: &HashSet<String>,
) -> Option<(String, BytePos)> {
    let Expr::Call(call) = expr else { return None };
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(m) = &**callee else {
        return None;
    };
    if !is_pathname_receiver_sets(&m.obj, pathname_aliases, url_provenanced) {
        return None;
    }
    let MemberProp::Ident(method) = &m.prop else {
        return None;
    };
    if method.sym != "match" {
        return None;
    }
    let [arg] = &call.args[..] else { return None };
    if arg.spread.is_some() {
        return None;
    }
    let Expr::Lit(Lit::Regex(re)) = &*arg.expr else {
        return None;
    };
    let path = regex_to_route_path(re.exp.as_str(), re.flags.as_str())?;
    Some((path, re.span.lo))
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

pub(super) fn is_pathname_receiver_sets(
    expr: &Expr,
    pathname_aliases: &HashSet<String>,
    url_provenanced: &HashSet<String>,
) -> bool {
    match expr {
        Expr::Ident(id) => pathname_aliases.contains(id.sym.as_str()),
        Expr::Member(_) => is_pathname_member(expr, url_provenanced),
        _ => false,
    }
}

pub(super) fn is_pathname_receiver(expr: &Expr, ctx: &FnCtx) -> bool {
    is_pathname_receiver_sets(expr, &ctx.pathname_aliases, &ctx.url_provenanced)
}

pub(super) fn is_method_receiver(expr: &Expr, ctx: &FnCtx) -> bool {
    match expr {
        Expr::Ident(id) => ctx.method_aliases.contains(id.sym.as_str()),
        Expr::Member(_) => is_method_member(expr, &ctx.request_idents),
        _ => false,
    }
}
