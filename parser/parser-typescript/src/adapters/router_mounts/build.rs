//! Fragment building (pass 2) for `router_mounts` — classifies each var-decl chain, statement,
//! and `export default` chain onto the right fragment. See the parent module doc for the spec.

use std::collections::{HashMap, HashSet};

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    CallExpr, Callee, ExportDefaultExpr, Expr, ExprOrSpread, ExprStmt, Lit, MemberProp, Pat,
    VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{ImportMap, RouterMountEntry, RouterMountFragment};

use super::chain::{unwrap_expr, walk_chain, ChainRoot};
use super::guard::{judge_guard_arg, AUTH_GUARDED_ATTR_KEY};
use super::use_classify::classify_use_call;

/// Pass 2: walks the module in source order, classifying entries onto the right fragment.
pub(super) struct FragmentBuilder<'a> {
    pub(super) cm: &'a SourceMap,
    pub(super) imports: &'a ImportMap,
    pub(super) receivers: &'a HashSet<String>,
    /// Subset of `receivers` recognized via Express shapes — gates the `.use`-as-`Mount` rule.
    pub(super) express_receivers: &'a HashSet<String>,
    pub(super) fragments: Vec<RouterMountFragment>,
    pub(super) index: HashMap<String, usize>,
}

impl Visit for FragmentBuilder<'_> {
    fn visit_var_declarator(&mut self, d: &VarDeclarator) {
        if let (Pat::Ident(bi), Some(init)) = (&d.name, &d.init) {
            let mut calls = Vec::new();
            if matches!(
                walk_chain(init, &mut calls, self.imports),
                ChainRoot::NewHono | ChainRoot::ExpressInit
            ) {
                let name = bi.id.sym.to_string();
                self.push_entries(&name, &calls);
                return;
            }
        }
        d.visit_children_with(self);
    }

    fn visit_expr_stmt(&mut self, n: &ExprStmt) {
        let mut calls = Vec::new();
        if let ChainRoot::Ident(name) = walk_chain(&n.expr, &mut calls, self.imports) {
            if self.receivers.contains(&name) {
                self.push_entries(&name, &calls);
                return;
            }
        }
        n.visit_children_with(self);
    }

    fn visit_export_default_expr(&mut self, n: &ExportDefaultExpr) {
        let mut calls = Vec::new();
        match walk_chain(&n.expr, &mut calls, self.imports) {
            // `is_express` is supplied directly here (not looked up in `express_receivers` by
            // name) because `"default"` is a synthesized fragment name, never a real identifier
            // pass 1 could have registered — looking it up would always miss and silently drop
            // `export default Router().use('/api', api)`'s `.use` mount.
            ChainRoot::NewHono => self.push_entries_as("default", &calls, false),
            ChainRoot::ExpressInit => self.push_entries_as("default", &calls, true),
            ChainRoot::Ident(name) if self.receivers.contains(&name) => {
                self.push_entries(&name, &calls);
            }
            _ => {}
        }
    }
}

impl FragmentBuilder<'_> {
    /// Index of `name`'s fragment, creating it (first-appearance order) on first entry.
    fn frag_idx(&mut self, name: &str) -> usize {
        if let Some(&i) = self.index.get(name) {
            return i;
        }
        let i = self.fragments.len();
        self.fragments.push(RouterMountFragment {
            name: name.to_string(),
            entries: Vec::new(),
        });
        self.index.insert(name.to_string(), i);
        i
    }

    /// Classifies `calls` and appends survivors onto `name`'s fragment — created only if at least
    /// one entry survives. Express-vocabulary status is looked up from `express_receivers` by
    /// `name`; see `push_entries_as` for the synthesized-name case where that lookup can't work.
    fn push_entries(&mut self, name: &str, calls: &[&CallExpr]) {
        let is_express = self.express_receivers.contains(name);
        self.push_entries_as(name, calls, is_express);
    }

    /// Same as `push_entries`, but `is_express` is supplied directly instead of looked up by
    /// `name` — needed for the synthesized `"default"` fragment name (a fresh
    /// `export default Router()...`/`export default express()...`/`export default new Hono()...`
    /// expression with no binding), which pass 1 never registers in `express_receivers` since it
    /// isn't a real identifier.
    fn push_entries_as(&mut self, name: &str, calls: &[&CallExpr], is_express: bool) {
        let entries: Vec<RouterMountEntry> = calls
            .iter()
            .flat_map(|c| self.classify_call(c, is_express))
            .collect();
        if entries.is_empty() {
            return;
        }
        let idx = self.frag_idx(name);
        self.fragments[idx].entries.extend(entries);
    }

    /// Classifies one call link: `.get|post|put|patch|delete` → `Verb` (0 or 1 entries), `.route` →
    /// `Mount` (any receiver, 0 or 1 entries), `.use` → `Mount`/`ScopedAttr` (Express receivers only,
    /// 0, 1, or several entries — see the `"use"` arm below). Anything else, or an unresolvable
    /// argument shape, yields an empty `Vec` (skip just this entry).
    fn classify_call(&self, call: &CallExpr, is_express: bool) -> Vec<RouterMountEntry> {
        let Callee::Expr(callee) = &call.callee else {
            return Vec::new();
        };
        let Expr::Member(m) = unwrap_expr(callee) else {
            return Vec::new();
        };
        let MemberProp::Ident(method) = &m.prop else {
            return Vec::new();
        };
        // The method identifier's own span, not the call's — a chained call's span starts at the
        // chain's root, so this keeps each entry's line accurate.
        let line = crate::line_of(self.cm, method.span.lo);
        match method.sym.as_str() {
            // Lowercase spelling of a `zzop_core::HTTP_KEY_VERBS` verb (T1: the verb set lives in
            // core) — the `.get(path, handler)` registration vocabulary — OR Express's `.all(path,
            // handler)`, which registers ONE handler for EVERY method (a catch-all), expanded below to
            // one entry per verb so it is not invisible and its mutating surface reaches
            // `mutating-route-no-auth`.
            verb if verb == "all"
                || zzop_core::HTTP_KEY_VERBS
                    .iter()
                    .any(|v| v.to_ascii_lowercase() == verb) =>
            {
                // A route registration always carries a handler argument; a single-argument call
                // (e.g. Express's `app.get('view engine')`) is a config getter, not a route.
                if call.args.len() < 2 {
                    return Vec::new();
                }
                let Some(path) = string_lit_arg(call.args.first()) else {
                    return Vec::new();
                };
                let handler = call
                    .args
                    .last()
                    .and_then(|a| handler_name(unwrap_expr(&a.expr)));
                // A middleware argument between the path and the last/handler arg
                // (`router.post('/x', requireAuth, handler)`) is judged for guard vocabulary —
                // route-level, as opposed to router-level (`.use`) — via the same helper.
                let attr_keys = if call.args.len() > 2
                    && call.args[1..call.args.len() - 1]
                        .iter()
                        .any(|a| judge_guard_arg(unwrap_expr(&a.expr)))
                {
                    vec![AUTH_GUARDED_ATTR_KEY.to_string()]
                } else {
                    Vec::new()
                };
                // `HTTP_KEY_VERBS` are already uppercase; a single verb uppercases its own spelling.
                let methods: Vec<String> = if verb == "all" {
                    zzop_core::HTTP_KEY_VERBS
                        .iter()
                        .map(|v| v.to_string())
                        .collect()
                } else {
                    vec![verb.to_uppercase()]
                };
                methods
                    .into_iter()
                    .map(|method| RouterMountEntry::Verb {
                        method,
                        path: path.clone(),
                        handler: handler.clone(),
                        line,
                        attr_keys: attr_keys.clone(),
                    })
                    .collect()
            }
            "route" => {
                let Some(prefix) = string_lit_arg(call.args.first()) else {
                    return Vec::new();
                };
                let Some(ident_arg) = call.args.get(1) else {
                    return Vec::new();
                };
                let Expr::Ident(id) = unwrap_expr(&ident_arg.expr) else {
                    return Vec::new();
                };
                let ident = id.sym.to_string();
                let specifier = self.imports.get(&ident).map(|b| b.specifier.clone());
                vec![RouterMountEntry::Mount {
                    prefix,
                    ident,
                    specifier,
                    attr_keys: Vec::new(),
                }]
            }
            // Express mounts sub-routers via `.use(prefixLit, subRouter)`, gated on Express
            // vocabulary since Hono's `.use` is always middleware — see `use_classify`'s module
            // doc for the full 1-arg/2-arg/multi-arg shape spec and the guard-name judgment it
            // layers on top (a RECOGNIZED guard name/callee mints `attr_keys`/`ScopedAttr` rather
            // than being silently dropped).
            "use" if is_express => classify_use_call(call, line, self.imports),
            _ => Vec::new(),
        }
    }
}

/// A handler argument's display name: a plain identifier (`handler`) or a dotted member chain
/// (`api.getUserInfo`) — the two shapes route registrations pass by reference; downstream
/// `IoProvide::symbol` consumers rely on the dotted form. Anything else (inline handlers, other
/// calls) → None. Also reused by `guard::judge_guard_arg` to read a middleware argument's dotted
/// callee/identifier text for the guard-name judgment.
pub(super) fn handler_name(e: &Expr) -> Option<String> {
    match e {
        Expr::Ident(id) => Some(id.sym.to_string()),
        Expr::This(_) => Some("this".to_string()),
        Expr::Member(m) => {
            let MemberProp::Ident(prop) = &m.prop else {
                return None;
            };
            let obj = handler_name(unwrap_expr(&m.obj))?;
            Some(format!("{obj}.{}", prop.sym))
        }
        _ => None,
    }
}

pub(super) fn string_lit_arg(arg: Option<&ExprOrSpread>) -> Option<String> {
    match unwrap_expr(&arg?.expr) {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}
