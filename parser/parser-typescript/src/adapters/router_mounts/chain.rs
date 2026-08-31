//! Chain walking + receiver recognition (pass 1) for `router_mounts` — see the parent module doc
//! for the recognizer spec.

use std::collections::HashSet;

use swc_core::ecma::ast::{
    AssignOp, BindingIdent, CallExpr, Callee, Expr, MemberProp, NewExpr, Pat, TsEntityName, TsType,
    TsTypeAnn, VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::ImportMap;

/// A method-call chain's root expression, classified for receiver/entry purposes.
pub(super) enum ChainRoot {
    /// Rooted at `new Hono(...)` (any generics) — a var-decl init or `export default` expression.
    NewHono,
    /// Rooted at `express()`, `express.Router()`, or an import-gated bare `Router()` call (see
    /// `is_express_router_import_call`) — kept separate from `NewHono` since the two vocabularies
    /// diverge on the `.use` mount rule.
    ExpressInit,
    /// Rooted at a bare identifier — an existing (possibly receiver) reference.
    Ident(String),
    /// Anything else — out of scope.
    Other,
}

/// Walks a member-call chain (`x.a(...).b(...)`) down to its root, collecting each call link in
/// source order. swc nests an earlier chain step inside the next call's `callee.obj`, so
/// recursing into the receiver before pushing the current call yields calls in source order.
/// `imports` gates the import-only `Router()` receiver shape (see `is_express_router_import_call`).
pub(super) fn walk_chain<'e>(
    expr: &'e Expr,
    calls: &mut Vec<&'e CallExpr>,
    imports: &ImportMap,
) -> ChainRoot {
    match unwrap_expr(expr) {
        Expr::Call(call) => {
            // `express()`/`express.Router()`/an import-gated `Router()` are the chain's root, not
            // a link — checked first so neither gets pushed onto `calls` and neither recurses
            // further.
            if is_express_call(call) || is_express_router_import_call(call, imports) {
                return ChainRoot::ExpressInit;
            }
            let Callee::Expr(callee) = &call.callee else {
                return ChainRoot::Other;
            };
            let Expr::Member(m) = unwrap_expr(callee) else {
                return ChainRoot::Other;
            };
            let root = walk_chain(&m.obj, calls, imports);
            calls.push(call);
            root
        }
        Expr::New(new_expr) => {
            if is_hono_new(new_expr) {
                ChainRoot::NewHono
            } else {
                ChainRoot::Other
            }
        }
        // A plain assignment CHAIN carries its value rightwards, so the root of `var app =
        // module.exports = express()` is what `express()` returns — the CommonJS spelling of
        // "build the app and export it in one statement", which `expressjs/express` uses in its
        // own `examples/multi-router/index.js`. Without this the binding is not a receiver at
        // all, and losing THAT loses the mounts registered on it: measured on that example, two
        // routers mounted at `/api/v1` and `/api/v2` both composed at their own keys and collided
        // into a false `duplicate-route` for `GET /` and `GET /users`.
        //
        // Restricted to `=`. A compound assignment (`x ||= express()`) does not mean the binding
        // holds that value, and a receiver minted on a guess is the direction this parser refuses.
        //
        // THREE callers see this arm, not the one that motivated it. The var-decl caller is the
        // measured case above. `visit_export_default_expr` gains nothing new (an `export default`
        // and a `module.exports =` do not co-occur in one statement). The third is
        // `FragmentBuilder::visit_expr_stmt`, and it gains a real shape: a bare
        // `module.exports = app.use("/api", api);` statement used to walk to `Other`, so the
        // receiver was never recognised and THAT `.use` mount was dropped -- the same loss as the
        // var-decl case, one statement form over. It now resolves rightwards to `Ident("app")`,
        // which that caller already gates on `self.receivers`, so nothing is minted that pass 1
        // did not already register. The empty-chain shape (`module.exports = app;`) resolves to
        // the same identifier and pushes an empty `calls`, which is a no-op by construction.
        Expr::Assign(a) if a.op == AssignOp::Assign => walk_chain(&a.right, calls, imports),
        Expr::Ident(id) => ChainRoot::Ident(id.sym.to_string()),
        _ => ChainRoot::Other,
    }
}

/// `new Hono(...)` / `new Hono<T>(...)` — generics never affect the callee.
fn is_hono_new(n: &NewExpr) -> bool {
    matches!(unwrap_expr(&n.callee), Expr::Ident(id) if id.sym == "Hono")
}

/// `express(...)` or `express.Router(...)` — the two Express receiver-init shapes, checked as a
/// chain root (see `walk_chain`).
fn is_express_call(call: &CallExpr) -> bool {
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    match unwrap_expr(callee) {
        Expr::Ident(id) => id.sym == "express",
        Expr::Member(m) => {
            matches!(unwrap_expr(&m.obj), Expr::Ident(id) if id.sym == "express")
                && matches!(&m.prop, MemberProp::Ident(p) if p.sym == "Router")
        }
        _ => false,
    }
}

/// A bare `Router(...)` call whose callee identifier resolves, via this file's `ImportMap`, to
/// the imported name `Router` from module specifier `'express'` — the named-import Express
/// idiom (`import { Router } from 'express'; const router = Router();`), including aliases
/// (`import { Router as R } from 'express'`). Gated on the import map — a bare `Router()` with no
/// such import never matches, same precision discipline as the rest of this recognizer (`Router`
/// alone is far too generic a name to trust without a structural/import signal).
fn is_express_router_import_call(call: &CallExpr, imports: &ImportMap) -> bool {
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Ident(id) = unwrap_expr(callee) else {
        return false;
    };
    imports
        .get(id.sym.as_str())
        .is_some_and(|b| b.original == "Router" && b.specifier == "express")
}

/// Pass 1: collects every receiver identifier. `express_names` is the subset recognized via
/// Express shapes, kept separate since only Express vocabulary gets the `.use` mount rule;
/// `names` still contains every Express receiver too. `imports` gates the import-only `Router()`
/// receiver shape.
pub(super) struct ReceiverCollector<'a> {
    pub(super) names: HashSet<String>,
    pub(super) express_names: HashSet<String>,
    pub(super) imports: &'a ImportMap,
}

impl Visit for ReceiverCollector<'_> {
    fn visit_var_declarator(&mut self, d: &VarDeclarator) {
        if let (Pat::Ident(bi), Some(init)) = (&d.name, &d.init) {
            let mut calls = Vec::new();
            match walk_chain(init, &mut calls, self.imports) {
                ChainRoot::NewHono => {
                    self.names.insert(bi.id.sym.to_string());
                }
                ChainRoot::ExpressInit => {
                    let name = bi.id.sym.to_string();
                    self.names.insert(name.clone());
                    self.express_names.insert(name);
                }
                _ => {}
            }
        }
        d.visit_children_with(self);
    }

    fn visit_binding_ident(&mut self, n: &BindingIdent) {
        // A receiver this file did not CONSTRUCT is only knowable from its type, and the commonest
        // shape by far is a parameter: `export function registerRoutes(app: Express)`. Until
        // 2026-08-16 this arm knew Hono alone, so every route registered on a passed-in Express app
        // was invisible — measured on a 14-route Express tree, 5 routes vanished and the downstream
        // consequence was not silence but confident wrong answers (three "no matching provide"
        // warnings for routes that were right there, and a posture census reporting 4 of 7 as a total).
        //
        // The admitted Express spellings are the two the framework's own typings give an application:
        // `Express` (what `express()` returns) and `Application` (`express.Application`). `Router` is
        // deliberately NOT admitted: as a bare type name it belongs to vue-router and react-router as
        // often as to express, and a wrong receiver mints wrong routes — the direction this parser
        // refuses. A router received as a parameter therefore still needs its own evidence.
        let Some(ty) = type_ref_name(n.type_ann.as_deref()) else {
            return;
        };
        match ty.as_str() {
            "Hono" => {
                self.names.insert(n.id.sym.to_string());
            }
            // Express vocabulary, so `.use(...)` on this receiver is a Mount like any other Express app.
            "Express" | "Application" => {
                let name = n.id.sym.to_string();
                self.names.insert(name.clone());
                self.express_names.insert(name);
            }
            _ => {}
        }
    }
}

/// A type annotation's single-identifier type name (e.g. `: Hono`).
fn type_ref_name(ann: Option<&TsTypeAnn>) -> Option<String> {
    let ann = ann?;
    if let TsType::TsTypeRef(tr) = &*ann.type_ann {
        if let TsEntityName::Ident(id) = &tr.type_name {
            return Some(id.sym.to_string());
        }
    }
    None
}

/// Strip value wrappers (`as const`/`as T`, parens, `satisfies T`, `!`) down to the real expression.
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
