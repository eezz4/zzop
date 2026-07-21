//! NestJS route-scoped **middleware auth** extraction — `MiddlewareConsumer.forRoutes(...)` route
//! coverage, for the `mutating-route-no-auth` rule's decorator/annotation auth exemption (a third producer
//! alongside the NestJS `@UseGuards` and Spring method-security ones).
//!
//! ## The shape
//! A NestJS module's `configure(consumer: MiddlewareConsumer)` binds middleware to routes:
//! ```ignore
//! consumer.apply(AuthMiddleware).forRoutes(
//!   {path: 'articles/:slug/comments', method: RequestMethod.POST},
//!   {path: 'articles', method: RequestMethod.POST});
//! ```
//! The middleware runs BEFORE the handler regardless of what the handler's body calls, so — exactly like
//! `@UseGuards` and route-level Express middleware — it is invisible to the call-graph BFS. This extractor
//! returns the `(METHOD, normalized-path)` patterns an AUTH-named middleware covers, so the engine can
//! match them against route provides and exempt those routes.
//!
//! ## Auth gate (why false-clears don't happen)
//! Only an `apply(...)` whose middleware NAME looks like auth ([`name_looks_like_auth`]:
//! `auth`/`guard`/`jwt`/`session`/`token`, case-insensitive) contributes. A
//! `consumer.apply(LoggerMiddleware).forRoutes(...)` binds a NON-auth middleware and is ignored, so a
//! logging/CORS middleware never spuriously clears a genuinely unguarded route. This is an exemption
//! producer for a security rule: emitting FEWER patterns only fails to exempt (the finding stays), never
//! clears a route that lacks real auth.
//!
//! This list OVERLAPS but is NOT identical to the rule's own `DEFAULT_AUTH_GUARD_PATTERN` (it adds `jwt`,
//! the near-universal Nest auth-middleware stem, and omits stems like `verify`/`permission`/`role` that
//! describe a guard's ACTION more than a middleware's name) — a deliberately narrow, parser-local
//! heuristic, not a shared vocabulary. Like that pattern, the broad `session`/`token` stems also match a
//! few non-authn middlewares (`SessionMiddleware`, `CsrfTokenMiddleware`); accepted as the same
//! established tradeoff, and bounded because the exemption still requires a route-PATTERN match.
//!
//! ## What is parsed (and what is not)
//! Each `forRoutes` argument that is an object literal `{path: <string>, method: RequestMethod.<VERB>}`
//! yields `(<VERB>, <path>)`; a bare string argument (`forRoutes('articles')`) yields `("*", 'articles')`
//! (NestJS applies it to every method of that path); an omitted `method` also means `"*"`. A path's `:param`
//! segments are normalized to `{}` to match `http_interface_key`. A `forRoutes(SomeController)` /
//! `forRoutes('*')` / wildcard-path form is NOT resolved here (the covered routes aren't decidable from
//! this call alone) — never guessed, so it simply fails to exempt (a conservative miss, the safe direction).

use swc_core::ecma::ast::{Callee, Expr, Lit, MemberProp, ObjectLit, Prop, PropName, PropOrSpread};
use swc_core::ecma::visit::{Visit, VisitWith};

/// A route pattern an auth middleware covers: `(method, path)` where `method` is an uppercase HTTP verb or
/// `"*"` (any method), and `path` is `http_interface_key`-normalized (`:slug` -> `{}`, no leading slash).
pub type ForRoutesPattern = (String, String);

/// Extract the `(method, path)` patterns covered by an auth-named middleware via
/// `consumer.apply(<Auth…>).forRoutes(...)` in this file. Empty (never panics) on an unparseable file or a
/// file with no such chain. Test files are skipped (their middleware wiring is not deployed coupling).
pub fn extract_nest_forroutes_guarded(rel: &str, text: &str) -> Vec<ForRoutesPattern> {
    if zzop_core::is_test_file(rel) {
        return Vec::new();
    }
    // Cheap pre-skip: no `forRoutes` token -> nothing to find (avoids parsing every TS file).
    if !text.contains("forRoutes") {
        return Vec::new();
    }
    let Some((_cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let mut c = ForRoutesCollector { out: Vec::new() };
    module.visit_with(&mut c);
    c.out
}

struct ForRoutesCollector {
    out: Vec<ForRoutesPattern>,
}

impl Visit for ForRoutesCollector {
    fn visit_call_expr(&mut self, call: &swc_core::ecma::ast::CallExpr) {
        // Match `<recv>.forRoutes(<args>)` where `<recv>` is `<x>.apply(<Auth…>)`.
        if let Callee::Expr(callee) = &call.callee {
            if let Expr::Member(member) = &**callee {
                if member_prop_is(&member.prop, "forRoutes") {
                    if let Expr::Call(apply_call) = &*member.obj {
                        if apply_targets_auth_middleware(apply_call) {
                            for arg in &call.args {
                                self.collect_route_arg(&arg.expr);
                            }
                        }
                    }
                }
            }
        }
        call.visit_children_with(self); // nested chains / other calls
    }
}

impl ForRoutesCollector {
    fn collect_route_arg(&mut self, arg: &Expr) {
        match arg {
            // `{path: 'x', method: RequestMethod.POST}`
            Expr::Object(obj) => {
                if let Some(path) = object_path(obj) {
                    let method = object_method(obj).unwrap_or_else(|| "*".to_string());
                    self.out.push((method, normalize_route_path(&path)));
                }
            }
            // bare string `'articles'` -> every method of that path
            Expr::Lit(Lit::Str(s)) => {
                let path = s.value.as_str().unwrap_or_default();
                self.out.push(("*".to_string(), normalize_route_path(path)));
            }
            _ => {} // a controller ref / wildcard / spread — never guessed
        }
    }
}

/// True when the `apply(...)` call's callee ends in `.apply` and at least one argument is an identifier
/// whose name looks like auth middleware.
fn apply_targets_auth_middleware(apply_call: &swc_core::ecma::ast::CallExpr) -> bool {
    let Callee::Expr(callee) = &apply_call.callee else {
        return false;
    };
    let Expr::Member(m) = &**callee else {
        return false;
    };
    if !member_prop_is(&m.prop, "apply") {
        return false;
    }
    apply_call.args.iter().any(|a| match &*a.expr {
        Expr::Ident(id) => name_looks_like_auth(id.sym.as_ref()),
        _ => false,
    })
}

/// The `path:` string value of an object literal, if present.
fn object_path(obj: &ObjectLit) -> Option<String> {
    obj.props.iter().find_map(|p| {
        let PropOrSpread::Prop(prop) = p else {
            return None;
        };
        let Prop::KeyValue(kv) = &**prop else {
            return None;
        };
        if !prop_key_is(&kv.key, "path") {
            return None;
        }
        match &*kv.value {
            Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
            _ => None,
        }
    })
}

/// The `method:` HTTP verb of an object literal (`RequestMethod.POST` -> `POST`, or a bare string), if
/// present. `RequestMethod.ALL` -> `"*"`.
fn object_method(obj: &ObjectLit) -> Option<String> {
    obj.props.iter().find_map(|p| {
        let PropOrSpread::Prop(prop) = p else {
            return None;
        };
        let Prop::KeyValue(kv) = &**prop else {
            return None;
        };
        if !prop_key_is(&kv.key, "method") {
            return None;
        }
        let verb = match &*kv.value {
            // `RequestMethod.POST`
            Expr::Member(m) => match &m.prop {
                MemberProp::Ident(id) => id.sym.to_string(),
                _ => return None,
            },
            Expr::Lit(Lit::Str(s)) => s.value.as_str().unwrap_or_default().to_string(),
            _ => return None,
        };
        Some(if verb.eq_ignore_ascii_case("all") {
            "*".to_string()
        } else {
            verb.to_uppercase()
        })
    })
}

/// Normalize a NestJS route path to the `http_interface_key` path shape: drop a leading `/`, and replace
/// every `:param` segment with `{}` (matching how a controller route's own key is built).
fn normalize_route_path(path: &str) -> String {
    let trimmed = path.trim_start_matches('/');
    trimmed
        .split('/')
        .map(|seg| if seg.starts_with(':') { "{}" } else { seg })
        .collect::<Vec<_>>()
        .join("/")
}

/// Auth-middleware name heuristic — the same vocabulary family the rule's guard pattern uses.
fn name_looks_like_auth(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["auth", "guard", "jwt", "session", "token"]
        .iter()
        .any(|w| lower.contains(w))
}

fn member_prop_is(prop: &MemberProp, name: &str) -> bool {
    matches!(prop, MemberProp::Ident(id) if id.sym.as_ref() == name)
}

fn prop_key_is(key: &PropName, name: &str) -> bool {
    match key {
        PropName::Ident(id) => id.sym.as_ref() == name,
        PropName::Str(s) => s.value.as_str() == Some(name),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
