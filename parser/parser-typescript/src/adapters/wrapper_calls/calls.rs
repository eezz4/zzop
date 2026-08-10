//! Call recognizer for `wrapper_calls` (`WrapperCallFragment`) — see the parent module doc for the
//! recognizer spec.

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{CallExpr, Callee, Expr, Lit, MemberProp, Tpl};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{ImportMap, WrapperCallFragment};

/// Whole-module walk collecting every candidate wrapper call site — see module doc's call recognizer.
pub(super) struct CallCollector<'a> {
    pub(super) cm: &'a SourceMap,
    pub(super) imports: &'a ImportMap,
    pub(super) out: &'a mut Vec<WrapperCallFragment>,
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
        // Two callee shapes resolve to a wrapper name: a bare identifier, and a member call on a
        // NAMESPACE import (`import * as api from './client'; api.get('/x')`). The second was out of
        // scope and it is the dominant shape in the wild — measured 19 of 19 call sites on
        // `corpus/oss/fe-svelte`, with zero named imports, so the whole file read as callless.
        //
        // It needs no new resolution machinery: `imports.rs` already records the namespace binding
        // with `original: "*"` and its specifier, the member name IS the wrapper name, and the
        // receiver supplies the specifier the def side joins on. The receiver MUST be a namespace
        // import of this file — a plain local object (`svc.get('/a')`) stays out, because treating any
        // `x.get(...)` as a wrapper call would sweep in every helper that happens to expose a `get`.
        let (callee_name, ns_specifier) = match &**callee {
            Expr::Ident(id) => (id.sym.to_string(), None),
            Expr::Member(m) => {
                let (Expr::Ident(recv), MemberProp::Ident(prop)) = (&*m.obj, &m.prop) else {
                    return None;
                };
                let binding = self.imports.get(recv.sym.as_ref())?;
                if binding.original != "*" {
                    return None; // a named/default import receiver is a value, not a module namespace
                }
                (prop.sym.to_string(), Some(binding.specifier.clone()))
            }
            _ => return None,
        };

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

        // A namespace receiver already named the module; otherwise the callee's own import binding does.
        let specifier =
            ns_specifier.or_else(|| self.imports.get(&callee_name).map(|b| b.specifier.clone()));
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
/// `resolve_url` applies.
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
