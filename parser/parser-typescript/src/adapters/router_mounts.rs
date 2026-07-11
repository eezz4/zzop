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
//! `express.Router()` / an import-gated bare `Router()`, `.use()` as mount) are independent
//! vocabularies feeding the same types through the same compose pass — a new framework costs
//! vocabulary only. Name-dependence stays confined to the recognizer gate, same precision
//! discipline as `trpc_router.rs`'s factory gate: `.get(...)` alone is far too common (axios, Map,
//! cache clients) to act on without a structural router signal — likewise a bare `Router()` call
//! is only trusted once this file's `ImportMap` confirms the callee resolves to the imported name
//! `Router` from module specifier `'express'` (see `is_express_router_import_call`); this is what
//! lets `import { Router } from 'express'; const router = Router();` — the canonical
//! controller-file idiom in Express codebases (e.g. gothinkster's node-express-realworld,
//! dogfood round 9) — join the same Express vocabulary as `express.Router()`.
//!
//! Recognition is swc-AST-based, so chained builders — including ones spanning several router
//! hops in a large real-world monorepo — are first-class, unlike a line-anchored regex.
//!
//! ## Implementation notes
//! - Two passes: pass 1 (`ReceiverCollector`) finds every receiver identifier — bound to
//!   `new Hono(...)` (bare or chain root), typed `: Hono`, an import-gated `Router()` call, or
//!   configured by name. Pass 2 (`FragmentBuilder`) walks again in source order, classifying each
//!   var-decl chain, statement, and `export default` chain onto the right fragment.
//! - `walk_chain` recurses a call chain down to its root; recursing before pushing the current
//!   call naturally yields calls in source order. It takes the file's `ImportMap` so the
//!   import-gated `Router()` chain root (`is_express_router_import_call`) can be recognized at
//!   any depth — bare receiver, chain root (`Router().use(a).use(b)`), or `export default`.
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
use zzop_core::{ImportMap, RouterMountEntry, RouterMountFragment};

/// Extract one file's router-mount fragments. Pure; parses `text` with the crate's swc pipeline.
/// Returns an empty vec for files with no recognized router.
///
/// Recognizer spec (Hono vocabulary + Express vocabulary + configured names):
/// - **Receivers**: an identifier bound to `new Hono(...)` (bare or chain root, any generics); a
///   function parameter typed `: Hono`; an identifier bound to `express()`, `express.Router()`,
///   or a bare `Router()` call whose callee resolves via this file's `ImportMap` to the imported
///   name `Router` from module specifier `'express'` (aliases like `import { Router as R } from
///   'express'` included; a `Router()` call with no such import is NOT a receiver — never
///   bare-name-matched) — all tracked as EXPRESS vocabulary, which matters for the `.use` mount
///   rules below; any identifier in `router_names` (config allowlist, vocabulary-agnostic); or
///   `export default new Hono()...` / `export default express()...` / `export default Router()...`
///   chains with no binding → fragment name `"default"`.
/// - **Entries** collected from both chained calls and separate statements (`recv.get('/a', h);`)
///   where `recv` is a receiver.
/// - `.get|post|put|patch|delete(pathLit, ...)` → `Verb` (method uppercased), requiring ≥2
///   arguments. A non-string-literal path skips just that entry. `.all`/`.on`/other members are
///   ignored; `.use` is ignored unless the receiver is Express vocabulary.
/// - `.route(prefixLit, identArg)` → `Mount` (any receiver). For an Express-vocabulary receiver,
///   `.use(prefixLit, identArg)` → `Mount` with that prefix, and `.use(identArg)` (exactly one
///   identifier argument) → `Mount` with prefix `"/"` (a prefix-less "mount at root", e.g.
///   `Router().use(subRouter)` in a `routes.ts`-style aggregation file). A non-identifier single
///   argument (`app.use(cors())`, `app.use(bodyParser.json())`, `app.use(express.static(...))`)
///   is SKIPPED, not mistaken for a mount. Non-literal prefix or a non-identifier second arg (in
///   the 2-argument form) also skips the entry. `specifier` resolves from this file's imports
///   when `identArg`'s name is an imported binding — same mechanism `.route` already uses.
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
        imports: &imports,
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
fn walk_chain<'e>(expr: &'e Expr, calls: &mut Vec<&'e CallExpr>, imports: &ImportMap) -> ChainRoot {
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
struct ReceiverCollector<'a> {
    names: HashSet<String>,
    express_names: HashSet<String>,
    imports: &'a ImportMap,
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
            // Lowercase spelling of a `zzop_core::HTTP_KEY_VERBS` verb (T1: the verb set lives in
            // core) — the `.get(path, handler)` registration vocabulary.
            verb if zzop_core::HTTP_KEY_VERBS
                .iter()
                .any(|v| v.to_ascii_lowercase() == verb) =>
            {
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
            //
            // A single-argument `.use(ident)` is the routes.ts aggregation idiom
            // (`Router().use(controllerA).use(controllerB)`) — a prefix-less mount at "/", which
            // `join_prefix` in the compose pass treats as a pure passthrough (no double slash). A
            // single non-identifier argument (`app.use(cors())`, `app.use(express.static(...))`)
            // is middleware, not a mount, and is skipped. A BARE-identifier middleware arg
            // (`app.use(helmet)`, `app.use(errorHandler)`) still mints a Mount that fails to
            // resolve at compose (middleware modules are not router fragments) — the same
            // accepted conservatism cost as the two-arg middleware case above.
            "use" if is_express => {
                if call.args.len() == 1 {
                    let arg = call.args.first()?;
                    let Expr::Ident(id) = unwrap_expr(&arg.expr) else {
                        return None;
                    };
                    let ident = id.sym.to_string();
                    let specifier = self.imports.get(&ident).map(|b| b.specifier.clone());
                    return Some(RouterMountEntry::Mount {
                        prefix: "/".to_string(),
                        ident,
                        specifier,
                    });
                }
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
    //! Hono's `.use` never mounting) — plus the named-import `Router()` vocabulary (bare/aliased
    //! import gate, no-import non-recognition, single-arg `.use` prefix-less mount, non-identifier
    //! single-arg `.use` skip, and `Router()` as a chain root incl. `export default`).
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

    // --- Express named-import `Router()` vocabulary (dogfood round 9 gaps A/B/C) ---

    #[test]
    fn named_import_router_controller_recognizes_verbs_with_middleware_arg() {
        // The gothinkster node-express-realworld controller idiom: a bare `Router()` receiver
        // from a named `import { Router } from 'express'`, verb registrations carrying a
        // middleware argument between the path and the handler.
        let src = concat!(
            "import { Router } from 'express';\n",
            "const router = Router();\n",
            "router.get('/articles', auth.optional, listArticles);\n",
            "router.post('/articles', auth.required, createArticle);\n",
            "export default router;\n"
        );
        let out = extract_router_mount_fragments("articleController.ts", src, &[]);
        assert_eq!(
            frag(&out, "router").entries,
            vec![
                RouterMountEntry::Verb {
                    method: "GET".into(),
                    path: "/articles".into(),
                    handler: Some("listArticles".into()),
                    line: 3,
                },
                RouterMountEntry::Verb {
                    method: "POST".into(),
                    path: "/articles".into(),
                    handler: Some("createArticle".into()),
                    line: 4,
                },
            ]
        );
        assert!(
            out.iter().all(|f| f.name != "default"),
            "named binding must win over a synthesized \"default\" fragment: {out:?}"
        );
    }

    #[test]
    fn aliased_router_import_is_recognized_bare_router_without_import_is_not() {
        let aliased = concat!(
            "import { Router as R } from 'express';\n",
            "const r = R();\n",
            "r.get('/x', h);\n"
        );
        let out = extract_router_mount_fragments("r.ts", aliased, &[]);
        assert_eq!(
            frag(&out, "r").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/x".into(),
                handler: Some("h".into()),
                line: 3,
            }],
            "an aliased `Router as R` import must still gate `R()` as an Express receiver: {out:?}"
        );

        let bare = "const router = Router();\nrouter.get('/x', h);\n";
        let out2 = extract_router_mount_fragments("r.ts", bare, &[]);
        assert!(
            out2.is_empty(),
            "a bare `Router()` with no express import must not be recognized: {out2:?}"
        );
    }

    #[test]
    fn routes_aggregation_single_arg_use_mounts_at_root_default_export_mounts_prefix() {
        // The RealWorld `routes.ts` aggregation shape: `Router()` as a chain root bound to a var
        // (`.use(ident)` mounts each controller at "/"), and `Router()` as a chain root under
        // `export default` (`.use(prefixLit, ident)` mounts the aggregated router under a prefix).
        let src = concat!(
            "import { Router } from 'express';\n",
            "import a from './controllers/a';\n",
            "import b from './controllers/b';\n",
            "const api = Router().use(a).use(b);\n",
            "export default Router().use('/api', api);\n"
        );
        let out = extract_router_mount_fragments("routes.ts", src, &[]);
        assert_eq!(
            frag(&out, "api").entries,
            vec![
                RouterMountEntry::Mount {
                    prefix: "/".into(),
                    ident: "a".into(),
                    specifier: Some("./controllers/a".into()),
                },
                RouterMountEntry::Mount {
                    prefix: "/".into(),
                    ident: "b".into(),
                    specifier: Some("./controllers/b".into()),
                },
            ]
        );
        assert_eq!(
            frag(&out, "default").entries,
            vec![RouterMountEntry::Mount {
                prefix: "/api".into(),
                ident: "api".into(),
                specifier: None,
            }],
            "export default Router().use('/api', api) must mount \"api\" under \"/api\": {out:?}"
        );
    }

    #[test]
    fn non_identifier_single_arg_use_calls_are_skipped_not_mistaken_for_mounts() {
        let src = concat!(
            "import { Router } from 'express';\n",
            "import express from 'express';\n",
            "import cors from 'cors';\n",
            "import bodyParser from 'body-parser';\n",
            "const app = Router();\n",
            "app.use(cors());\n",
            "app.use(bodyParser.json());\n",
            "app.use(express.static('/public'));\n",
            "app.get('/ok', h);\n"
        );
        let out = extract_router_mount_fragments("app.ts", src, &[]);
        assert_eq!(
            frag(&out, "app").entries,
            vec![RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/ok".into(),
                handler: Some("h".into()),
                line: 9,
            }],
            "cors()/bodyParser.json()/express.static(...) must not mint a Mount: {out:?}"
        );
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
