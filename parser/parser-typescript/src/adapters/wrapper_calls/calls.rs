//! Call recognizer for `wrapper_calls` (`WrapperCallFragment`) — see the parent module doc for the
//! recognizer spec.

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{CallExpr, Callee, Expr, Lit, Tpl};
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
