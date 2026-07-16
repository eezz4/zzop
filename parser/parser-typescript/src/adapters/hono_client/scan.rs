//! Receiver + base-path scanning for `hono_client` — collects hc-derived receivers, the file's
//! first `hc(...)` call, and resolves the base-path expression. See the parent module doc.

use std::collections::HashSet;

use swc_core::common::BytePos;
use swc_core::ecma::ast::{
    AssignExpr, AssignTarget, CallExpr, Callee, ClassDecl, Expr, Lit, MemberProp, NewExpr, Pat,
    Prop, PropName, PropOrSpread, SimpleAssignTarget, Tpl, VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};

/// Info about the FIRST `hc(...)`/`hc<T>(...)` call in the file, used to derive the base path.
pub(super) struct FirstCall {
    pub(super) span_lo: BytePos,
    /// Directly resolved from a literal/template argument.
    pub(super) resolved: Option<String>,
    /// `(one-hop property name, enclosing class name)` for the `new ClassName({ prop: <expr> })` trace.
    pub(super) trace: Option<(String, String)>,
}

/// Single pass collecting: hc-derived receivers (both routes), and the file's first hc() call info.
pub(super) struct ClientScanner<'a> {
    pub(super) hc_idents: &'a HashSet<String>,
    pub(super) class_stack: Vec<String>,
    pub(super) ident_receivers: HashSet<String>,
    pub(super) this_field_receivers: HashSet<String>,
    pub(super) first_call: Option<FirstCall>,
    /// Total `hc(...)` calls seen — 2+ means the file has no single attributable base.
    pub(super) hc_call_count: u32,
}

impl Visit for ClientScanner<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        self.class_stack.push(n.ident.sym.to_string());
        n.visit_children_with(self);
        self.class_stack.pop();
    }

    fn visit_var_declarator(&mut self, d: &VarDeclarator) {
        if let (Pat::Ident(bi), Some(init)) = (&d.name, &d.init) {
            if is_hc_call(unwrap_expr(init), self.hc_idents) {
                self.ident_receivers.insert(bi.id.sym.to_string());
            }
        }
        d.visit_children_with(self);
    }

    fn visit_assign_expr(&mut self, n: &AssignExpr) {
        if let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &n.left {
            if let (Expr::This(_), MemberProp::Ident(field)) = (unwrap_expr(&m.obj), &m.prop) {
                if is_hc_call(unwrap_expr(&n.right), self.hc_idents) {
                    self.this_field_receivers.insert(field.sym.to_string());
                }
            }
        }
        n.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if is_hc_callee(call, self.hc_idents) {
            self.hc_call_count += 1;
            let lo = call.span.lo();
            let earlier = self
                .first_call
                .as_ref()
                .map(|fc| lo < fc.span_lo)
                .unwrap_or(true);
            if earlier {
                if let Some(arg) = call.args.first() {
                    let resolved = resolve_literal_or_template(&arg.expr);
                    let trace = if resolved.is_none() {
                        one_hop_target(&arg.expr).zip(self.class_stack.last().cloned())
                    } else {
                        None
                    };
                    self.first_call = Some(FirstCall {
                        span_lo: lo,
                        resolved,
                        trace,
                    });
                }
            }
        }
        call.visit_children_with(self); // recurse into nested calls
    }
}

/// True when `expr` (already unwrapped) is a call whose callee is a plain identifier in `hc_idents`.
fn is_hc_call(expr: &Expr, hc_idents: &HashSet<String>) -> bool {
    matches!(expr, Expr::Call(call) if is_hc_callee(call, hc_idents))
}

fn is_hc_callee(call: &CallExpr, hc_idents: &HashSet<String>) -> bool {
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    matches!(unwrap_expr(callee), Expr::Ident(id) if hc_idents.contains(id.sym.as_ref()))
}

/// The property name to look up on a same-file `new <EnclosingClass>({ <prop>: <expr> })` call.
fn one_hop_target(expr: &Expr) -> Option<String> {
    match unwrap_expr(expr) {
        Expr::Ident(id) => Some(id.sym.to_string()),
        Expr::Member(m) => match &m.prop {
            MemberProp::Ident(name) => Some(name.sym.to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// Finds the first `new <class_name>({ ..., <prop_name>: <expr>, ... })` and resolves `<expr>` via
/// [`resolve_literal_or_template`] — the "one hop" the base-path trace is allowed to take.
pub(super) struct NewExprPropFinder<'a> {
    pub(super) class_name: &'a str,
    pub(super) prop_name: &'a str,
    pub(super) result: Option<String>,
}

impl Visit for NewExprPropFinder<'_> {
    fn visit_new_expr(&mut self, n: &NewExpr) {
        let is_target_class =
            matches!(unwrap_expr(&n.callee), Expr::Ident(id) if id.sym == self.class_name);
        if self.result.is_none() && is_target_class {
            if let Some(Expr::Object(obj)) = n
                .args
                .as_ref()
                .and_then(|args| args.first())
                .map(|a| unwrap_expr(&a.expr))
            {
                for prop in &obj.props {
                    let PropOrSpread::Prop(p) = prop else {
                        continue;
                    };
                    let Prop::KeyValue(kv) = &**p else {
                        continue;
                    };
                    let name = match &kv.key {
                        PropName::Ident(i) => i.sym.to_string(),
                        PropName::Str(s) => s.value.as_str().unwrap_or_default().to_string(),
                        _ => continue,
                    };
                    if name == self.prop_name {
                        self.result = resolve_literal_or_template(&kv.value);
                    }
                }
            }
        }
        n.visit_children_with(self);
    }
}

/// Resolves a string-literal or template-literal base-path expression, `None` for any other shape.
fn resolve_literal_or_template(expr: &Expr) -> Option<String> {
    match unwrap_expr(expr) {
        Expr::Lit(Lit::Str(s)) => resolve_str_literal(s.value.as_str().unwrap_or_default()),
        Expr::Tpl(t) => resolve_template(t),
        _ => None,
    }
}

/// `/api/auth` -> itself; `https://host/api/auth` -> `/api/auth`; anything else (no leading slash,
/// no scheme) -> `None`, never guessed.
fn resolve_str_literal(v: &str) -> Option<String> {
    if v.starts_with('/') {
        Some(v.to_string())
    } else if let Some(idx) = v.to_ascii_lowercase().find("://") {
        let rest = &v[idx + 3..];
        rest.find('/').map(|i| rest[i..].to_string())
    } else {
        None
    }
}

/// Concatenates quasis with each interpolation as a `{}` placeholder, then takes the substring
/// starting at the first `/`. `None` if that substring still contains `{}` or has no `/` at all.
fn resolve_template(t: &Tpl) -> Option<String> {
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
    // Scheme strip — same `://` handling as `resolve_str_literal`: a full URL written as a backtick
    // string must yield the PATH part, not fold the host into the key.
    let idx = if let Some(scheme_idx) = s.find("://") {
        let after_scheme = scheme_idx + 3;
        s[after_scheme..].find('/').map(|i| after_scheme + i)?
    } else {
        s.find('/')?
    };
    let candidate = &s[idx..];
    if candidate.contains("{}") {
        None
    } else {
        Some(candidate.to_string())
    }
}

/// Strip wrappers between an expression and its real value: `... as const`, `(...)`, `... satisfies T`, `...!`.
pub(super) fn unwrap_expr(e: &Expr) -> &Expr {
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
