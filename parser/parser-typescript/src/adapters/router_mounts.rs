//! Per-file router-mount fragments — the provide-side sibling of `trpc_router.rs`'s
//! `RouterFragment`. A code-registered router is often split across files: verb registrations
//! live on sub-routers (`twoFactorRoute.post('/setup', h)`), sub-routers are mounted with a
//! prefix (`.route('/two-factor', twoFactorRoute)`), and the app itself is mounted again
//! (`app.route('/api/auth', auth)`). The real URL only exists once every hop is joined, which no
//! per-file pass can do — so each file projects a fragment here, and the engine composes
//! whole-tree `http` provides at assemble time (`compose_router_mount_provides`).
//!
//! The fragment SHAPE (`Verb`, `Mount`) is framework-agnostic; only the RECOGNIZER is
//! framework-specific. Hono (`new Hono()`, `.route()`) and Express (`express()` /
//! `express.Router()`, `.use()` as mount) are independent vocabularies feeding the same types
//! through the same compose pass — a new framework costs vocabulary only. Name-dependence stays
//! confined to the recognizer gate, same precision discipline as `trpc_router.rs`'s factory gate:
//! `.get(...)` alone is far too common (axios, Map, cache clients) to act on without a structural
//! router signal.
//!
//! Recognition is swc-AST-based, so chained builders — including ones spanning several router
//! hops in a large real-world monorepo — are first-class, unlike a line-anchored regex.
//!
//! ## Implementation notes
//! - Two passes: pass 1 (`ReceiverCollector`) finds every receiver identifier — bound to
//!   `new Hono(...)` (bare or chain root), typed `: Hono`, or configured by name. Pass 2
//!   (`FragmentBuilder`) walks again in source order, classifying each var-decl chain, statement,
//!   and `export default` chain onto the right fragment.
//! - `walk_chain` recurses a call chain down to its root; recursing before pushing the current
//!   call naturally yields calls in source order.
//! - `Verb::line` uses the `.get`/`.post`/... identifier's own span, not the call's: swc gives a
//!   chained call the same start position as the chain's root, which would misreport the line on
//!   a multi-line chain otherwise.

use std::collections::{HashMap, HashSet};

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    BindingIdent, CallExpr, Callee, ExportDefaultExpr, Expr, ExprOrSpread, ExprStmt, Lit,
    MemberProp, NewExpr, Pat, TsEntityName, TsType, TsTypeAnn, VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zpz_core::{ImportMap, RouterMountEntry, RouterMountFragment};

/// Extract one file's router-mount fragments. Pure; parses `text` with the crate's swc pipeline.
/// Returns an empty vec for files with no recognized router.
///
/// Recognizer spec (Hono vocabulary + Express vocabulary + configured names):
/// - **Receivers**: an identifier bound to `new Hono(...)` (bare or chain root, any generics); a
///   function parameter typed `: Hono`; an identifier bound to `express()` or `express.Router()`
///   (tracked as EXPRESS vocabulary — matters only for the `.use` mount rule below); any
///   identifier in `router_names` (config allowlist, vocabulary-agnostic); or
///   `export default new Hono()...` with no binding → fragment name `"default"`.
/// - **Entries** collected from both chained calls and separate statements (`recv.get('/a', h);`)
///   where `recv` is a receiver.
/// - `.get|post|put|patch|delete(pathLit, ...)` → `Verb` (method uppercased), requiring ≥2
///   arguments. A non-string-literal path skips just that entry. `.all`/`.on`/other members are
///   ignored; `.use` is ignored unless the receiver is Express vocabulary.
/// - `.route(prefixLit, identArg)` → `Mount` (any receiver). `.use(prefixLit, identArg)` →
///   `Mount` only for an Express-vocabulary receiver. Non-literal prefix, a single argument, or a
///   non-identifier second arg skips the entry. `specifier` resolves from this file's imports
///   when `identArg`'s name is an imported binding.
/// - A receiver with zero surviving entries produces no fragment. Output order: fragments in
///   first-appearance order, entries in source order.
pub fn extract_router_mount_fragments(
    rel: &str,
    text: &str,
    router_names: &[&str],
) -> Vec<RouterMountFragment> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let imports = crate::parse_imports(rel, text);

    let mut receivers: HashSet<String> = router_names.iter().map(|s| s.to_string()).collect();
    let mut rc = ReceiverCollector {
        names: HashSet::new(),
        express_names: HashSet::new(),
    };
    module.visit_with(&mut rc);
    receivers.extend(rc.names.iter().cloned());
    let express_receivers = rc.express_names;

    let mut builder = FragmentBuilder {
        cm: &cm,
        imports: &imports,
        receivers: &receivers,
        express_receivers: &express_receivers,
        fragments: Vec::new(),
        index: HashMap::new(),
    };
    module.visit_with(&mut builder);
    builder.fragments
}

/// A method-call chain's root expression, classified for receiver/entry purposes.
enum ChainRoot {
    /// Rooted at `new Hono(...)` (any generics) — a var-decl init or `export default` expression.
    NewHono,
    /// Rooted at `express()` or `express.Router()` — kept separate from `NewHono` since the two
    /// vocabularies diverge on the `.use` mount rule.
    ExpressInit,
    /// Rooted at a bare identifier — an existing (possibly receiver) reference.
    Ident(String),
    /// Anything else — out of scope.
    Other,
}

/// Walks a member-call chain (`x.a(...).b(...)`) down to its root, collecting each call link in
/// source order. swc nests an earlier chain step inside the next call's `callee.obj`, so
/// recursing into the receiver before pushing the current call yields calls in source order.
fn walk_chain<'e>(expr: &'e Expr, calls: &mut Vec<&'e CallExpr>) -> ChainRoot {
    match unwrap_expr(expr) {
        Expr::Call(call) => {
            // `express()`/`express.Router()` are the chain's root, not a link — checked first so
            // neither gets pushed onto `calls` and neither recurses further.
            if is_express_call(call) {
                return ChainRoot::ExpressInit;
            }
            let Callee::Expr(callee) = &call.callee else {
                return ChainRoot::Other;
            };
            let Expr::Member(m) = unwrap_expr(callee) else {
                return ChainRoot::Other;
            };
            let root = walk_chain(&m.obj, calls);
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

/// Pass 1: collects every receiver identifier. `express_names` is the subset recognized via
/// Express shapes, kept separate since only Express vocabulary gets the `.use` mount rule;
/// `names` still contains every Express receiver too.
struct ReceiverCollector {
    names: HashSet<String>,
    express_names: HashSet<String>,
}

impl Visit for ReceiverCollector {
    fn visit_var_declarator(&mut self, d: &VarDeclarator) {
        if let (Pat::Ident(bi), Some(init)) = (&d.name, &d.init) {
            let mut calls = Vec::new();
            match walk_chain(init, &mut calls) {
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
        if type_ref_name(n.type_ann.as_deref()).as_deref() == Some("Hono") {
            self.names.insert(n.id.sym.to_string());
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

/// Pass 2: walks the module in source order, classifying entries onto the right fragment.
struct FragmentBuilder<'a> {
    cm: &'a SourceMap,
    imports: &'a ImportMap,
    receivers: &'a HashSet<String>,
    /// Subset of `receivers` recognized via Express shapes — gates the `.use`-as-`Mount` rule.
    express_receivers: &'a HashSet<String>,
    fragments: Vec<RouterMountFragment>,
    index: HashMap<String, usize>,
}

impl Visit for FragmentBuilder<'_> {
    fn visit_var_declarator(&mut self, d: &VarDeclarator) {
        if let (Pat::Ident(bi), Some(init)) = (&d.name, &d.init) {
            let mut calls = Vec::new();
            if matches!(
                walk_chain(init, &mut calls),
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
        if let ChainRoot::Ident(name) = walk_chain(&n.expr, &mut calls) {
            if self.receivers.contains(&name) {
                self.push_entries(&name, &calls);
                return;
            }
        }
        n.visit_children_with(self);
    }

    fn visit_export_default_expr(&mut self, n: &ExportDefaultExpr) {
        let mut calls = Vec::new();
        match walk_chain(&n.expr, &mut calls) {
            ChainRoot::NewHono => self.push_entries("default", &calls),
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
    /// one entry survives.
    fn push_entries(&mut self, name: &str, calls: &[&CallExpr]) {
        let is_express = self.express_receivers.contains(name);
        let entries: Vec<RouterMountEntry> = calls
            .iter()
            .filter_map(|c| self.classify_call(c, is_express))
            .collect();
        if entries.is_empty() {
            return;
        }
        let idx = self.frag_idx(name);
        self.fragments[idx].entries.extend(entries);
    }

    /// Classifies one call link: `.get|post|put|patch|delete` → `Verb`, `.route` → `Mount` (any
    /// receiver), `.use` → `Mount` (Express receivers only). Anything else, or an unresolvable
    /// argument shape, yields `None` (skip just this entry).
    fn classify_call(&self, call: &CallExpr, is_express: bool) -> Option<RouterMountEntry> {
        let Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let Expr::Member(m) = unwrap_expr(callee) else {
            return None;
        };
        let MemberProp::Ident(method) = &m.prop else {
            return None;
        };
        match method.sym.as_str() {
            verb @ ("get" | "post" | "put" | "patch" | "delete") => {
                // A route registration always carries a handler argument; a single-argument call
                // (e.g. Express's `app.get('view engine')`) is a config getter, not a route.
                if call.args.len() < 2 {
                    return None;
                }
                let path = string_lit_arg(call.args.first())?;
                let handler = call
                    .args
                    .last()
                    .and_then(|a| handler_name(unwrap_expr(&a.expr)));
                Some(RouterMountEntry::Verb {
                    method: verb.to_uppercase(),
                    path,
                    handler,
                    // The method identifier's own span, not the call's — a chained call's span
                    // starts at the chain's root, so this keeps each entry's line accurate.
                    line: crate::line_of(self.cm, method.span.lo),
                })
            }
            "route" => {
                let prefix = string_lit_arg(call.args.first())?;
                let ident_arg = call.args.get(1)?;
                let Expr::Ident(id) = unwrap_expr(&ident_arg.expr) else {
                    return None;
                };
                let ident = id.sym.to_string();
                let specifier = self.imports.get(&ident).map(|b| b.specifier.clone());
                Some(RouterMountEntry::Mount {
                    prefix,
                    ident,
                    specifier,
                })
            }
            // Express mounts sub-routers via `.use(prefixLit, subRouter)`, gated on Express
            // vocabulary since Hono's `.use` is always middleware. Known limit: a plain-ident
            // second arg that is actually middleware (e.g. `app.use('/api', logger)`) still
            // mints a `Mount` that fails to resolve at compose — an accepted cost of this
            // recognizer's existing conservatism.
            "use" if is_express => {
                let prefix = string_lit_arg(call.args.first())?;
                let ident_arg = call.args.get(1)?;
                let Expr::Ident(id) = unwrap_expr(&ident_arg.expr) else {
                    return None;
                };
                let ident = id.sym.to_string();
                let specifier = self.imports.get(&ident).map(|b| b.specifier.clone());
                Some(RouterMountEntry::Mount {
                    prefix,
                    ident,
                    specifier,
                })
            }
            _ => None,
        }
    }
}

/// A handler argument's display name: a plain identifier (`handler`) or a dotted member chain
/// (`api.getUserInfo`) — the two shapes route registrations pass by reference; downstream
/// `IoProvide::symbol` consumers rely on the dotted form. Anything else (inline handlers, other
/// calls) → None.
fn handler_name(e: &Expr) -> Option<String> {
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

fn string_lit_arg(arg: Option<&ExprOrSpread>) -> Option<String> {
    match unwrap_expr(&arg?.expr) {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}

/// Strip value wrappers (`as const`/`as T`, parens, `satisfies T`, `!`) down to the real expression.
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

#[cfg(test)]
mod tests {
    //! Coverage: chained-builder entries (verb + mount, in source order), the last-arg handler
    //! rule, separate-statement style, `export default` fragments, cross-file `Mount` specifier
    //! resolution, `router_names` config, typed `: Hono` params, the "no structural signal"
    //! precision guard, non-literal path/prefix skip, ignored `.all`/`.use`, determinism, and the
    //! Express vocabulary (bare/chained receivers, `.use` mounts, the ≥2-arg verb guard, and
    //! Hono's `.use` never mounting).
    use super::*;

    fn frag<'a>(out: &'a [RouterMountFragment], name: &str) -> &'a RouterMountFragment {
        out.iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no fragment named {name:?} in {out:?}"))
    }

    #[test]
    fn auth_router_chained_builder_collects_verb_and_mount_entries() {
        let src = concat!(
            "import { sessionRoute } from './routes/session';\n",
            "export const auth = new Hono<HonoAuthContext>()\n",
            "  .use(async (c, next) => { await next(); })\n",
            "  .get('/csrf', csrfHandler)\n",
            "  .route('/', sessionRoute)\n",
            "  .route('/two-factor', twoFactorRoute);\n"
        );
        let out = extract_router_mount_fragments("auth.ts", src, &[]);
        assert_eq!(
            frag(&out, "auth").entries,
            vec![
                RouterMountEntry::Verb {
                    method: "GET".into(),
                    path: "/csrf".into(),
                    handler: Some("csrfHandler".into()),
                    line: 4,
                },
                RouterMountEntry::Mount {
                    prefix: "/".into(),
                    ident: "sessionRoute".into(),
                    specifier: Some("./routes/session".into()),
                },
                RouterMountEntry::Mount {
                    prefix: "/two-factor".into(),
                    ident: "twoFactorRoute".into(),
                    specifier: None,
                },
            ]
        );
    }

    #[test]
    fn sub_route_module_last_arg_is_the_handler() {
        let src = concat!(
            "export const twoFactorRoute = new Hono<T>()\n",
            "  .post('/setup', handler)\n",
            "  .post('/enable', sValidator('json', Schema), enableHandler);\n"
        );
        let out = extract_router_mount_fragments("two-factor.ts", src, &[]);
        assert_eq!(
            frag(&out, "twoFactorRoute").entries,
            vec![
                RouterMountEntry::Verb {
                    method: "POST".into(),
                    path: "/setup".into(),
                    handler: Some("handler".into()),
                    line: 2,
                },
                RouterMountEntry::Verb {
                    method: "POST".into(),
                    path: "/enable".into(),
                    handler: Some("enableHandler".into()),
                    line: 3,
                },
            ]
        );
    }

    #[test]
    fn separate_statement_style_named_binding_wins_over_default() {
        let src = concat!(
            "const route = new Hono();\n",
            "route.get('/envelope/:envelopeId/item', h);\n",
            "export default route;\n"
        );
        let out = extract_router_mount_fragments("routes.ts", src, &[]);
        assert_eq!(
            frag(&out, "route").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/envelope/:envelopeId/item".into(),
                handler: Some("h".into()),
                line: 2,
            }]
        );
        assert!(
            out.iter().all(|f| f.name != "default"),
            "a named binding must win over a synthesized \"default\" fragment: {out:?}"
        );
        assert_eq!(out.len(), 1, "{out:?}");
    }

    #[test]
    fn export_default_with_no_binding_yields_a_default_fragment() {
        let src = "export default new Hono().get('/y', h);\n";
        let out = extract_router_mount_fragments("default.ts", src, &[]);
        assert_eq!(
            frag(&out, "default").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/y".into(),
                handler: Some("h".into()),
                line: 1,
            }]
        );
    }

    #[test]
    fn top_level_mount_resolves_import_specifier() {
        let src = concat!(
            "import { auth } from '@example/auth-server';\n",
            "const app = new Hono();\n",
            "app.route('/api/auth', auth);\n"
        );
        let out = extract_router_mount_fragments("app.ts", src, &[]);
        assert_eq!(
            frag(&out, "app").entries,
            vec![RouterMountEntry::Mount {
                prefix: "/api/auth".into(),
                ident: "auth".into(),
                specifier: Some("@example/auth-server".into()),
            }]
        );
    }

    #[test]
    fn configured_router_names_receiver() {
        let src = "apiRoutes.get(\"/health\", healthHandler);\n";
        let out = extract_router_mount_fragments("health.ts", src, &["apiRoutes"]);
        assert_eq!(
            frag(&out, "apiRoutes").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/health".into(),
                handler: Some("healthHandler".into()),
                line: 1,
            }]
        );
    }

    #[test]
    fn hono_typed_parameter_is_a_receiver() {
        let src = concat!(
            "function register(app: Hono, handlers: AuthHandlerShape): void {\n",
            "  app.get('/x', h);\n",
            "}\n"
        );
        let out = extract_router_mount_fragments("register.ts", src, &[]);
        assert_eq!(
            frag(&out, "app").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/x".into(),
                handler: Some("h".into()),
                line: 2,
            }]
        );
    }

    #[test]
    fn unrelated_receivers_without_a_structural_signal_are_not_routes() {
        let src = concat!(
            "const cache = new Map();\n",
            "cache.get('key');\n",
            "axios.get('/url', handler);\n"
        );
        let out = extract_router_mount_fragments("cache.ts", src, &[]);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn non_literal_path_skips_only_that_entry_fragment_survives() {
        let src = concat!(
            "const route = new Hono();\n",
            "route.get(SOME_CONST, h).get('/ok', h2);\n"
        );
        let out = extract_router_mount_fragments("route.ts", src, &[]);
        assert_eq!(
            frag(&out, "route").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/ok".into(),
                handler: Some("h2".into()),
                line: 2,
            }]
        );
    }

    #[test]
    fn all_and_use_are_ignored() {
        let src = concat!(
            "const route = new Hono();\n",
            "route.all('/x', h);\n",
            "route.use(mw);\n",
            "route.get('/ok', h2);\n"
        );
        let out = extract_router_mount_fragments("route.ts", src, &[]);
        assert_eq!(
            frag(&out, "route").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/ok".into(),
                handler: Some("h2".into()),
                line: 4,
            }]
        );
    }

    #[test]
    fn express_router_use_mounts_a_sub_router() {
        let src = concat!(
            "const app = express();\n",
            "const router = express.Router();\n",
            "router.get('/users', listUsers);\n",
            "router.post('/users', createUser);\n",
            "app.use('/api', router);\n"
        );
        let out = extract_router_mount_fragments("app.ts", src, &[]);
        assert_eq!(
            frag(&out, "router").entries,
            vec![
                RouterMountEntry::Verb {
                    method: "GET".into(),
                    path: "/users".into(),
                    handler: Some("listUsers".into()),
                    line: 3,
                },
                RouterMountEntry::Verb {
                    method: "POST".into(),
                    path: "/users".into(),
                    handler: Some("createUser".into()),
                    line: 4,
                },
            ]
        );
        assert_eq!(
            frag(&out, "app").entries,
            vec![RouterMountEntry::Mount {
                prefix: "/api".into(),
                ident: "router".into(),
                specifier: None,
            }]
        );
    }

    #[test]
    fn express_use_mount_resolves_cross_file_specifier() {
        let src = concat!(
            "import { usersRouter } from './users';\n",
            "const app = express();\n",
            "app.use('/api/users', usersRouter);\n"
        );
        let out = extract_router_mount_fragments("app.ts", src, &[]);
        assert_eq!(
            frag(&out, "app").entries,
            vec![RouterMountEntry::Mount {
                prefix: "/api/users".into(),
                ident: "usersRouter".into(),
                specifier: Some("./users".into()),
            }]
        );
    }

    #[test]
    fn express_single_arg_call_is_a_config_getter_not_a_route() {
        let src = concat!(
            "const app = express();\n",
            "app.get('view engine');\n",
            "app.get('/health', h);\n"
        );
        let out = extract_router_mount_fragments("app.ts", src, &[]);
        assert_eq!(
            frag(&out, "app").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/health".into(),
                handler: Some("h".into()),
                line: 3,
            }],
            "the single-arg 'view engine' getter must not be a Verb: {out:?}"
        );
    }

    #[test]
    fn hono_single_arg_get_no_longer_emits_a_verb() {
        // Same ≥2-arg guard applied to Hono, proving it is vocabulary-neutral, not Express-only.
        let src = concat!(
            "const route = new Hono();\n",
            "route.get('/x');\n",
            "route.get('/ok', h);\n"
        );
        let out = extract_router_mount_fragments("route.ts", src, &[]);
        assert_eq!(
            frag(&out, "route").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/ok".into(),
                handler: Some("h".into()),
                line: 3,
            }]
        );
    }

    #[test]
    fn hono_use_still_never_mounts_even_with_ident_second_arg() {
        let src = concat!(
            "const app = new Hono();\n",
            "app.use('/path', someMiddleware);\n",
            "app.get('/ok', h);\n"
        );
        let out = extract_router_mount_fragments("app.ts", src, &[]);
        assert_eq!(
            frag(&out, "app").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/ok".into(),
                handler: Some("h".into()),
                line: 3,
            }],
            "Hono's .use must never mint a Mount, vocabulary separation from Express: {out:?}"
        );
    }

    #[test]
    fn express_router_chained_builder() {
        let src = "const r = express.Router().get('/a', h);\n";
        let out = extract_router_mount_fragments("r.ts", src, &[]);
        assert_eq!(
            frag(&out, "r").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/a".into(),
                handler: Some("h".into()),
                line: 1,
            }]
        );
    }

    #[test]
    fn bare_router_get_with_no_qualifying_binding_is_not_recognized() {
        let src = "router.get('/a', h);\n";
        let out = extract_router_mount_fragments("r.ts", src, &[]);
        assert!(out.is_empty(), "no vocabulary leak: {out:?}");
    }

    #[test]
    fn deterministic_across_repeated_extractions() {
        let src = concat!(
            "import { sessionRoute } from './routes/session';\n",
            "export const auth = new Hono<HonoAuthContext>()\n",
            "  .use(async (c, next) => { await next(); })\n",
            "  .get('/csrf', csrfHandler)\n",
            "  .route('/', sessionRoute)\n",
            "  .route('/two-factor', twoFactorRoute);\n"
        );
        let a = extract_router_mount_fragments("auth.ts", src, &[]);
        let b = extract_router_mount_fragments("auth.ts", src, &[]);
        assert_eq!(a, b);
    }

    #[test]
    fn empty_file_yields_no_fragments() {
        assert!(extract_router_mount_fragments("e.ts", "", &[]).is_empty());
    }
}
