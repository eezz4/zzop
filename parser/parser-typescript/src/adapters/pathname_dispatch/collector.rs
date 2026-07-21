// Module-level walk: finds every function-like node and every DO-vetoed class.

use std::collections::{HashMap, HashSet};

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    ArrowExpr, BlockStmt, BlockStmtOrExpr, Class, ClassDecl, ClassExpr, ClassMember, ClassMethod,
    Constructor, Expr, FnDecl, FnExpr, Function, MethodProp, ParamOrTsParamProp, Pat, PropName,
    TsParamPropParam, VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::IoProvide;

use super::ctx::{build_fn_ctx_seed, type_ann_is, BindingCollector, FnCtx};
use super::routes::RouteCollector;

pub(super) struct TopCollector<'a> {
    pub(super) cm: &'a SourceMap,
    pub(super) rel: &'a str,
    pub(super) out: Vec<IoProvide>,
    /// The binding name of an enclosing `const <name> = <arrow | anonymous function expr>`,
    /// consumed by the very next `visit_arrow_expr`/`visit_fn_expr` call (set right before
    /// recursing into a `VarDeclarator`'s children, cleared right after).
    pub(super) pending_name: Option<String>,
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
            pathname_match_routes: HashMap::new(),
            pathname_match_poisoned: HashSet::new(),
        };
        body.visit_with(&mut bindings);
        let ctx = FnCtx {
            symbol,
            request_idents,
            url_provenanced: bindings.url_provenanced,
            pathname_aliases: bindings.pathname_aliases,
            method_aliases: bindings.method_aliases,
            pathname_match_routes: bindings.pathname_match_routes,
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

// Durable Object veto

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
