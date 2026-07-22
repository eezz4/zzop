//! Verb collection: walks a resolved handler body ([`super::resolve::HandlerBody`]) for the three
//! structural verb signals — a `req.method`-shaped equality comparison, a `req.method`-discriminated
//! `switch`, and a `defaultHandler({ GET: …, POST: … })` call-argument object — scoped to that body
//! (including nested blocks/closures inside it), never the whole module.

use swc_core::ecma::ast::{
    BinExpr, BinaryOp, CallExpr, Callee, Expr, Lit, MemberProp, Prop, PropName, PropOrSpread,
    SwitchStmt,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::HTTP_KEY_VERBS;

use super::resolve::HandlerBody;

/// Sorted, deduped UPPERCASE verbs witnessed in `body` — see [`super::PagesApiHandlerScan::verbs`].
/// `request_ident` is the resolved handler's first-parameter name; `None` disables the
/// method-comparison/switch signals (an unresolvable parameter) while leaving the
/// `defaultHandler(…)` call signal — which needs no request identifier — active.
pub(super) fn collect_verbs(body: &HandlerBody, request_ident: Option<&str>) -> Vec<String> {
    let mut collector = VerbCollector {
        verbs: Vec::new(),
        request_ident,
    };
    match body {
        HandlerBody::Block(b) => b.visit_with(&mut collector),
        HandlerBody::Expr(e) => e.visit_with(&mut collector),
    }
    collector.verbs.sort();
    collector.verbs
}

/// Walks the resolved handler body collecting the three structural verb signals, scoped to that body
/// (including nested blocks/closures inside it) — never the whole module. Each signal is brace-scoped
/// by construction: a `switch`'s case labels are that switch's own, and only contribute when its
/// discriminant is the request identifier's `.method`; a `defaultHandler(…)`'s keys are that call's
/// own object arg.
struct VerbCollector<'a> {
    verbs: Vec<String>,
    request_ident: Option<&'a str>,
}

impl Visit for VerbCollector<'_> {
    fn visit_bin_expr(&mut self, n: &BinExpr) {
        if is_equality_op(n.op) {
            let lit = if self.is_method_member(&n.left) {
                Some(&n.right)
            } else if self.is_method_member(&n.right) {
                Some(&n.left)
            } else {
                None
            };
            if let Some(verb) = lit.and_then(|e| verb_literal(e)) {
                push_unique_verb(&mut self.verbs, verb);
            }
        }
        n.visit_children_with(self);
    }

    fn visit_switch_stmt(&mut self, n: &SwitchStmt) {
        if self.is_method_member(&n.discriminant) {
            for case in &n.cases {
                if let Some(verb) = case.test.as_deref().and_then(verb_literal) {
                    push_unique_verb(&mut self.verbs, verb);
                }
            }
        }
        n.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, n: &CallExpr) {
        if callee_is_default_handler(&n.callee) {
            for arg in &n.args {
                let Expr::Object(obj) = &*arg.expr else {
                    continue;
                };
                for prop in &obj.props {
                    if let PropOrSpread::Prop(p) = prop {
                        if let Some(verb) = verb_prop_key(p) {
                            push_unique_verb(&mut self.verbs, verb);
                        }
                    }
                }
            }
        }
        n.visit_children_with(self);
    }
}

impl VerbCollector<'_> {
    /// `<request_ident>.method` — a member access whose object is EXACTLY the resolved handler's
    /// first-parameter identifier (never hardcoded `req`/`request`) and whose property is `method`.
    fn is_method_member(&self, expr: &Expr) -> bool {
        let Some(target) = self.request_ident else {
            return false;
        };
        let Expr::Member(m) = expr else { return false };
        let Expr::Ident(obj) = &*m.obj else {
            return false;
        };
        if obj.sym != target {
            return false;
        }
        matches!(&m.prop, MemberProp::Ident(p) if p.sym == "method")
    }
}

fn is_equality_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::EqEqEq | BinaryOp::NotEqEq | BinaryOp::EqEq | BinaryOp::NotEq
    )
}

/// A string literal whose value is one of `HTTP_KEY_VERBS`, returned as its canonical `&'static str`.
fn verb_literal(expr: &Expr) -> Option<&'static str> {
    let Expr::Lit(Lit::Str(s)) = expr else {
        return None;
    };
    let value = s.value.as_str().unwrap_or_default();
    HTTP_KEY_VERBS.iter().copied().find(|&v| v == value)
}

/// An object-property key that is one of `HTTP_KEY_VERBS` (`GET:` / `"GET":` / `'GET':`).
fn verb_prop_key(prop: &Prop) -> Option<&'static str> {
    let key = match prop {
        Prop::KeyValue(kv) => &kv.key,
        Prop::Shorthand(id) => {
            let name = id.sym.as_str();
            return HTTP_KEY_VERBS.iter().copied().find(|&v| v == name);
        }
        _ => return None,
    };
    let name = match key {
        PropName::Ident(id) => id.sym.as_str(),
        PropName::Str(s) => s.value.as_str().unwrap_or_default(),
        _ => return None,
    };
    HTTP_KEY_VERBS.iter().copied().find(|&v| v == name)
}

/// A callee whose final name is `defaultHandler` (`defaultHandler(…)` or `x.defaultHandler(…)`).
fn callee_is_default_handler(callee: &Callee) -> bool {
    let Callee::Expr(e) = callee else {
        return false;
    };
    match &**e {
        Expr::Ident(id) => id.sym == "defaultHandler",
        Expr::Member(m) => matches!(&m.prop, MemberProp::Ident(p) if p.sym == "defaultHandler"),
        _ => false,
    }
}

fn push_unique_verb(verbs: &mut Vec<String>, verb: &str) {
    if !verbs.iter().any(|v| v == verb) {
        verbs.push(verb.to_string());
    }
}
