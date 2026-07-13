//! Controller-decorator-shape HTTP route PROVIDES extraction — swc-AST-based (mirrors
//! `zzop_parser_java::provides`'s Spring extractor: class-level prefix + method-level verb/path,
//! class gated on a controller marker, ambiguous-verb decorators skipped rather than guess-emitted).
//! Walks the real swc AST (`Class`/`Decorator`/`Function` nodes) instead of reconstructing an
//! annotation block from text. Generic over any framework using this decorator SHAPE — recognizes
//! both NestJS's own `@Controller`/`@Get`/... and `@n8n/decorators`'s structurally identical
//! `@RestController`/`@Get`/...; framework names live only in the vocabulary constants below
//! (`CONTROLLER_CLASS_GATES`, `METHOD_DECORATORS`), never in the extraction logic.
//!
//! ## Scope (v1)
//! - Class-level gate: `@Controller` or `@RestController` on a `class ... {}` declaration
//!   (`ClassDecl` only; a `ClassExpr` assignment is not detected). Both share the same argument
//!   shapes and prefix/version resolution (`controller_context`): bare, `()`, `('prefix')`, or
//!   `({ path: 'prefix' })`. A class with neither decorator yields no provides for any method.
//!   Decorators are matched by lexical name only (see "Known limits").
//! - A `@Controller({ ... })` prefix that is present but not a string literal skips the WHOLE
//!   controller — more conservative than Java's "default to empty prefix" fallback, since treating
//!   an unresolvable prefix as empty would mis-join every route under a wrong path (this repo's
//!   "never guess" IO convention — see `egress.rs`'s `resolve_url`). **Exception
//!   (`controller-prefix-ref-v1`):** when the class-level prefix arg itself is exactly a two-segment
//!   member expression (`RouteKey.Asset`) — the dotted shape `egress::const_map_fragment` keys its
//!   constant-map entries by — the controller is no longer skipped outright: this DEFERS resolution
//!   to assemble time instead of skipping (`extract_controller_prefix_route_fragments` projects the
//!   controller's methods as `zzop_core::ControllerPrefixRouteFragment`s rather than `IoProvide`s;
//!   `zzop_engine::analyze::compose`'s controller-prefix composer resolves `prefix_ref` against the
//!   project-wide merged const map, which can see the `enum`/`const` declaring it even when that
//!   declaration lives in another file). Any OTHER non-literal shape — a call, a template, a computed
//!   member, a deeper `A.B.C` chain, or the `{path: ref}` object form below — still skips the whole
//!   controller outright.
//! - Nest URI versioning: `{ path: 'x', version: '1' }` prefixes a `v<version>` segment ahead of the
//!   path. A non-literal `version` best-effort skips just that segment, not the whole controller.
//! - Method-level: `@Get`/`@Post`/`@Put`/`@Delete`/`@Patch` each imply their verb. Path comes from a
//!   bare decorator (empty path), a string literal, or an array of string literals (one provide per
//!   entry, mirroring Nest's own per-entry registration). A non-literal/mixed path skips the method.
//! - `@All` is deliberately skipped rather than guess-emitting one of the five verbs — mirrors
//!   `zzop_parser_java::provides`'s ambiguous bare `@RequestMapping` skip.
//! - Every decorator on a method is scanned for a route-verb name (or `@All`), not just the first,
//!   so other decorators (`@UseGuards`, `@ApiTags`, ...) and decorator order never matter.
//!
//! ## Known limits (v1 scope, not fixed)
//! - Lexical name matching only — import source is never verified, so a same-named decorator from an
//!   unrelated library plus a same-named class gate would false-positive (same tradeoff as the Java
//!   annotation extractor; the required double collision makes it vanishingly rare).
//! - Method-level `@Version()` overrides are not read.
//! - An array `path` prefix (`{ path: ['a','b'] }`) takes only the first literal entry, same
//!   "first wins" simplification as `zzop_parser_java::provides`'s `first_quoted_string`.
//! - Nested/child controllers, nested classes, `applyDecorators`, and inherited/abstract controller
//!   base classes are not detected — only a direct class-level decorator gates its own methods.
//!
//! ## NestJS `@UseGuards` decorator exemption (`extract_controller_guarded_lines`)
//! Detects `@UseGuards(...)` auth-guard coverage at class level (every route in that controller) or
//! method level (just that route). A decorator application is metadata, not a call edge, so it is
//! invisible to a call-graph BFS — see `zzop_rules_http::mutating_route_no_auth`'s module doc. A
//! returned line always matches a route `extract_controller_provides` would emit (same file/line).
//! Guard presence is checked by decorator name only, not argument identities.
//!
//! **Known residual:** NestJS's GLOBAL guards (`app.useGlobalGuards(...)`, or an `APP_GUARD`
//! provider) apply to every route in the app — a file-level signal this per-class extractor can't
//! see. A controller relying only on a global guard yields no guarded lines here and still
//! false-positives on the consuming rule.
//!
//! **Known residual — frameworks with no `@UseGuards` equivalent:** some frameworks sharing this
//! decorator shape invert NestJS's model — every route is authenticated by default, opting OUT via a
//! flag in the route decorator's options (e.g. `@n8n/decorators`'s `{ skipAuth: true }`) rather than
//! opting IN via a guard; reading that flag is out of scope (a "provide carries its own
//! auth-exemption" concept, not "decorator adds guard coverage"). `@n8n/decorators`'s `@Licensed(...)`
//! (a feature-flag gate, not identity) and `@GlobalScope(...)`/`@ProjectScope(...)` (permission-scope,
//! presupposing an already-authenticated caller) were both considered and rejected as `@UseGuards`
//! equivalents. Net effect: mutating routes on such a framework still false-positive on
//! `mutating-route-no-auth` unless their handler reaches a guard-vocabulary-named call.

use std::collections::HashSet;

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    ArrayLit, Callee, ClassDecl, ClassMember, ClassMethod, Decorator, Expr, ExprOrSpread, Lit,
    MemberProp, ObjectLit, Param, Pat, Prop, PropName, PropOrSpread, Str, TsEntityName, TsType,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_interface_key, ControllerPrefixRouteFragment, IoProvide, ProvideBodyShape};

/// Method-level route-decorator name -> the HTTP verb it implies. `All` is intentionally absent —
/// see module doc.
const METHOD_DECORATORS: &[(&str, &str)] = &[
    ("Get", "GET"),
    ("Post", "POST"),
    ("Put", "PUT"),
    ("Delete", "DELETE"),
    ("Patch", "PATCH"),
];

/// Extracts NestJS `@Controller`/`@Get`/`@Post`/... HTTP route `IoProvide`s from one TS file's raw
/// source — see module doc for the decorator shapes and gating rules. Returns an empty `Vec` (never
/// panics) on an unparseable file, same convention as every other swc-AST adapter in this crate.
pub fn extract_controller_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut c = ControllerCollector {
        cm: cm_ref,
        file: rel,
        out: Vec::new(),
    };
    module.visit_with(&mut c);
    c.out
}

struct ControllerCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    out: Vec<IoProvide>,
}

impl Visit for ControllerCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        if let Some(ControllerCtx::Literal { prefix }) = controller_context(&n.class.decorators) {
            for member in &n.class.body {
                if let ClassMember::Method(m) = member {
                    self.emit_method(&prefix, m);
                }
            }
        }
        // `ControllerCtx::DeferredRef` (a `RouteKey.Asset`-shaped prefix) emits no direct provides here
        // — see `extract_controller_prefix_route_fragments`.
        n.visit_children_with(self); // recurse — covers any nested class declarations
    }
}

impl ControllerCollector<'_> {
    fn emit_method(&mut self, prefix: &str, m: &ClassMethod) {
        let Some((verb, name, line, paths, body)) = method_route_facts(self.cm, m) else {
            return;
        };
        for path in paths {
            let full_path = format!("{prefix}/{path}");
            self.out.push(IoProvide {
                body: body.clone(),
                kind: "http".to_string(),
                key: http_interface_key(&verb, &full_path),
                file: self.file.to_string(),
                line,
                symbol: Some(name.clone()),
            });
        }
    }
}

/// Extracts controller-prefix route FRAGMENTS — the deferred-to-assemble counterpart of
/// `extract_controller_provides` for the `controller-prefix-ref-v1` exception (module doc): a
/// `@Controller(RouteKey.Asset)`-shaped (dotted member-expression) prefix cannot be resolved from this
/// one file alone, so each qualifying controller's methods are projected as
/// `zzop_core::ControllerPrefixRouteFragment`s instead of `IoProvide`s.
/// `zzop_engine::analyze::compose`'s controller-prefix composer resolves `prefix_ref` against the
/// project-wide merged const map (`egress::const_map_fragment`, which also folds string-valued `enum`
/// members) and emits the real `IoProvide`s at assemble time. A `@Controller('literal')` class
/// contributes nothing here (already fully resolved by `extract_controller_provides`); any other
/// non-literal prefix shape (call, template, computed member, deeper chain, `{path: ref}` object)
/// contributes nothing here either — same skip-whole-controller convention as
/// `extract_controller_provides`. Returns an empty `Vec` (never panics) on an unparseable file.
pub fn extract_controller_prefix_route_fragments(
    rel: &str,
    text: &str,
) -> Vec<ControllerPrefixRouteFragment> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut c = ControllerPrefixFragmentCollector {
        cm: cm_ref,
        out: Vec::new(),
    };
    module.visit_with(&mut c);
    c.out
}

struct ControllerPrefixFragmentCollector<'a> {
    cm: &'a SourceMap,
    out: Vec<ControllerPrefixRouteFragment>,
}

impl Visit for ControllerPrefixFragmentCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        if let Some(ControllerCtx::DeferredRef { prefix_ref }) =
            controller_context(&n.class.decorators)
        {
            for member in &n.class.body {
                if let ClassMember::Method(m) = member {
                    self.emit_fragment(&prefix_ref, m);
                }
            }
        }
        n.visit_children_with(self); // recurse — covers any nested class declarations
    }
}

impl ControllerPrefixFragmentCollector<'_> {
    fn emit_fragment(&mut self, prefix_ref: &str, m: &ClassMethod) {
        let Some((verb, name, line, paths, body)) = method_route_facts(self.cm, m) else {
            return;
        };
        for path in paths {
            self.out.push(ControllerPrefixRouteFragment {
                body: body.clone(),
                prefix_ref: prefix_ref.to_string(),
                verb: verb.clone(),
                path,
                line,
                symbol: Some(name.clone()),
            });
        }
    }
}

/// Detects NestJS `@UseGuards(...)` decorator coverage — see module doc "NestJS `@UseGuards` decorator
/// exemption". Returns the set of route-registration lines (matching `IoProvide::line` for whatever
/// `extract_controller_provides` would emit from the same file) covered by an explicit `@UseGuards(...)`
/// chain, either class-level or method-level. Empty set (never panics) on an unparseable file.
pub fn extract_controller_guarded_lines(rel: &str, text: &str) -> HashSet<u32> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return HashSet::new();
    };
    let mut c = GuardedLineCollector {
        cm: &cm,
        out: HashSet::new(),
    };
    module.visit_with(&mut c);
    c.out
}

struct GuardedLineCollector<'a> {
    cm: &'a SourceMap,
    out: HashSet<u32>,
}

impl Visit for GuardedLineCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        // Mirrors `emit_method`'s own class/method gating exactly, so a guarded line only appears
        // here if `extract_controller_provides` would also emit a real `IoProvide` for it.
        if let Some(_ctx) = controller_context(&n.class.decorators) {
            let class_guarded = has_use_guards(&n.class.decorators);
            for member in &n.class.body {
                if let ClassMember::Method(m) = member {
                    if class_guarded || has_use_guards(&m.function.decorators) {
                        if let Some((_, decorator, _)) = method_route(&m.function.decorators) {
                            self.out.insert(crate::line_of(self.cm, decorator.span.lo));
                        }
                    }
                }
            }
        }
        n.visit_children_with(self); // recurse — covers any nested class declarations
    }
}

fn has_use_guards(decorators: &[Decorator]) -> bool {
    decorators
        .iter()
        .any(|d| decorator_name(&d.expr).as_deref() == Some("UseGuards"))
}

/// A controller class's own routing context — either a fully resolved literal prefix (already
/// including the `v<version>` segment when `version` was a string literal — see module doc), or a
/// dotted member-expression reference DEFERRED to assemble-time resolution (`controller-prefix-ref-v1`
/// — see module doc's "Scope (v1)" exception).
enum ControllerCtx {
    Literal { prefix: String },
    DeferredRef { prefix_ref: String },
}

/// Class-level decorator names that gate a class as a controller — see module doc "Scope (v1)" for
/// why both are recognized.
const CONTROLLER_CLASS_GATES: &[&str] = &["Controller", "RestController"];

/// Scans a class's own decorators for `@Controller`/`@RestController` and returns its resolved
/// `ControllerCtx`, or `None` when neither is present or the prefix is unresolvable (see module doc).
fn controller_context(decorators: &[Decorator]) -> Option<ControllerCtx> {
    for d in decorators {
        let Some(name) = decorator_name(&d.expr) else {
            continue;
        };
        if !CONTROLLER_CLASS_GATES.contains(&name.as_str()) {
            continue;
        }
        return controller_ctx_from_expr(&d.expr);
    }
    None
}

// Bare `@Controller` and empty-parens `@Controller()` both yield an empty prefix.
fn controller_ctx_from_expr(expr: &Expr) -> Option<ControllerCtx> {
    let Expr::Call(call) = expr else {
        return Some(ControllerCtx::Literal {
            prefix: String::new(),
        });
    };
    let Some(arg) = call.args.first() else {
        return Some(ControllerCtx::Literal {
            prefix: String::new(),
        });
    };
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(ControllerCtx::Literal {
            prefix: str_value(s),
        }),
        Expr::Object(obj) => object_controller_ctx(obj),
        // A dotted two-segment member expression (`RouteKey.Asset`) is deferred to assemble time
        // rather than skipped — see module doc's `controller-prefix-ref-v1` exception. Any deeper
        // chain (`A.B.C`) or computed member falls through `simple_member_ref` to `None`, which this
        // `.map` propagates — same skip-whole-controller outcome as any other unrecognized shape.
        Expr::Member(_) => {
            simple_member_ref(&arg.expr).map(|prefix_ref| ControllerCtx::DeferredRef { prefix_ref })
        }
        _ => None, // dynamic prefix arg (call/template/...) — never guess, skip the whole controller
    }
}

/// A dotted two-segment member-expression reference (`RouteKey.Asset`) — exactly the shape
/// `egress::const_map_fragment`/`flatten` key their constant-map entries by. `None` for anything
/// deeper (`A.B.C`), computed (`A[x]`), or not identifier-rooted.
fn simple_member_ref(expr: &Expr) -> Option<String> {
    let Expr::Member(m) = expr else { return None };
    let Expr::Ident(obj) = &*m.obj else {
        return None;
    };
    let MemberProp::Ident(prop) = &m.prop else {
        return None;
    };
    Some(format!("{}.{}", obj.sym, prop.sym))
}

fn object_controller_ctx(obj: &ObjectLit) -> Option<ControllerCtx> {
    let mut path = String::new();
    let mut path_seen = false;
    let mut path_dynamic = false;
    let mut version: Option<String> = None;

    for prop in &obj.props {
        let PropOrSpread::Prop(p) = prop else {
            continue;
        };
        let Prop::KeyValue(kv) = &**p else {
            continue;
        };
        let Some(key) = prop_key_name(&kv.key) else {
            continue;
        };
        match key.as_str() {
            "path" => {
                path_seen = true;
                match first_literal_path(&kv.value) {
                    Some(p) => path = p,
                    None => path_dynamic = true,
                }
            }
            "version" => {
                if let Expr::Lit(Lit::Str(s)) = &*kv.value {
                    version = Some(str_value(s));
                }
                // non-literal version -> best-effort skip of just the version segment, not the
                // whole controller (see module doc).
            }
            _ => {}
        }
    }

    if path_seen && path_dynamic {
        return None; // an unresolvable `path` — never guess, skip the whole controller
    }

    let prefix = match version {
        Some(v) => format!("v{v}/{path}"),
        None => path,
    };
    Some(ControllerCtx::Literal { prefix })
}

/// A `path` attribute's value: a bare string literal, or ("first wins" — see module doc) the first
/// string-literal element of an array. `None` for anything else.
fn first_literal_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(Lit::Str(s)) => Some(str_value(s)),
        Expr::Array(ArrayLit { elems, .. }) => {
            let first = elems.first()?.as_ref()?;
            match &*first.expr {
                Expr::Lit(Lit::Str(s)) => Some(str_value(s)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `(verb, handler name, anchor line, resolved path(s), `@Body()` request-body contract)` — see
/// `method_route_facts`'s doc.
type MethodRouteFacts = (String, String, u32, Vec<String>, Option<ProvideBodyShape>);

/// One method's route facts, independent of its class's own prefix resolution — shared substrate for
/// both `ControllerCollector::emit_method` (literal-prefix provides) and
/// `ControllerPrefixFragmentCollector::emit_fragment` (deferred-prefix fragments): the handler name,
/// verb, anchor line, resolved path(s), and (`body-shape-v1`) the `@Body()` request-body contract. The
/// first four are `None` for the same reasons `method_route`/`method_name` individually return `None`
/// (no recognizable method key, no recognized route decorator, `@All`, or an unresolvable path); `body`
/// is independently `None`/`Some` per `method_body_shape`'s own never-guess rules and never vetoes the
/// rest of the tuple.
fn method_route_facts(cm: &SourceMap, m: &ClassMethod) -> Option<MethodRouteFacts> {
    let name = method_name(&m.key)?;
    let (verb, decorator, paths) = method_route(&m.function.decorators)?;
    let line = crate::line_of(cm, decorator.span.lo);
    let body = method_body_shape(&m.function.params);
    Some((verb, name, line, paths, body))
}

/// `body-shape-v1`: resolves a method's `@Body()` request-body contract from its parameter list.
/// `None` (never-guess) unless EXACTLY ONE parameter carries an `@Body`/`@Body(...)` decorator AND
/// that one param is fully capturable — a literal-or-absent decorator argument (`body_sub_key`) AND a
/// plain single-identifier type annotation (`capturable_dto_ref`). Zero `@Body` params, 2+ `@Body`
/// params, a non-literal decorator argument, or a non-capturable type annotation (qualified name,
/// generic, array, primitive keyword, missing annotation) all fall through to `None` — this method's
/// route keeps `body: None` rather than guessing a partial shape.
fn method_body_shape(params: &[Param]) -> Option<ProvideBodyShape> {
    let mut found: Option<&Param> = None;
    for p in params {
        let has_body = p
            .decorators
            .iter()
            .any(|d| decorator_name(&d.expr).as_deref() == Some("Body"));
        if has_body {
            if found.is_some() {
                return None; // 2+ @Body params on one method -- ambiguous, never guess
            }
            found = Some(p);
        }
    }
    let param = found?; // zero @Body params -- nothing to capture
    let body_decorator = param
        .decorators
        .iter()
        .find(|d| decorator_name(&d.expr).as_deref() == Some("Body"))?;
    let sub_key = body_sub_key(&body_decorator.expr)?;
    let dto_ref = capturable_dto_ref(&param.pat)?;
    Some(ProvideBodyShape {
        sub_key,
        dto_ref: Some(dto_ref),
        fields: Vec::new(),
        complete: false,
    })
}

/// An `@Body(...)` decorator's sub-key, tri-state: `Some(None)` = capturable, whole-body (bare `@Body`
/// or empty-parens `@Body()`); `Some(Some(key))` = capturable, keyed under `key` (first argument is a
/// string literal, e.g. `@Body('user')`); `None` = NOT capturable — a non-literal first argument
/// (`@Body(x)`) — never guess which sub-key it would have resolved to.
fn body_sub_key(expr: &Expr) -> Option<Option<String>> {
    let Expr::Call(call) = expr else {
        return Some(None); // bare `@Body` (not even called)
    };
    let Some(arg) = call.args.first() else {
        return Some(None); // `@Body()` -- whole body
    };
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(Some(str_value(s))),
        _ => None, // non-literal first arg -- never guess
    }
}

/// A `@Body()` param's type annotation as a capturable single-identifier DTO ref: `Pat::Ident` whose
/// type annotation is a plain `TsTypeRef` (`TsEntityName::Ident`, no type params). `None` for a
/// qualified name (`A.B`), a generic (`Foo<T>`), an array/tuple/primitive-keyword annotation, or a
/// missing annotation entirely — any of these makes the DTO shape unresolvable from this file alone.
fn capturable_dto_ref(pat: &Pat) -> Option<String> {
    let Pat::Ident(bi) = pat else { return None };
    let ann = bi.type_ann.as_deref()?;
    let TsType::TsTypeRef(tr) = &*ann.type_ann else {
        return None;
    };
    if tr.type_params.is_some() {
        return None; // a generic (`Foo<T>`) -- not a plain DTO reference
    }
    let TsEntityName::Ident(id) = &tr.type_name else {
        return None; // a qualified name (`A.B`) -- not a plain DTO reference
    };
    Some(id.sym.to_string())
}

/// Scans one method's decorators for a route-verb decorator (or `@All`) — see module doc for why
/// every decorator is scanned rather than just the first. Returns `(VERB, the matching Decorator,
/// resolved paths)`, or `None` when nothing recognized, `@All` is present, or the path is dynamic.
fn method_route(decorators: &[Decorator]) -> Option<(String, &Decorator, Vec<String>)> {
    for d in decorators {
        let Some(name) = decorator_name(&d.expr) else {
            continue;
        };
        if name == "All" {
            return None; // ambiguous verb — see module doc, mirrors Java's bare @RequestMapping skip
        }
        let Some((_, verb)) = METHOD_DECORATORS.iter().find(|(n, _)| *n == name) else {
            continue; // some other decorator (@UseGuards, @ApiTags, ...) — keep scanning
        };
        let paths = decorator_paths(&d.expr)?;
        return Some((verb.to_string(), d, paths));
    }
    None
}

/// A route decorator's resolved path(s): bare/`()` yields one empty-string path; a string literal
/// yields one path; an array yields one path per string-literal element (Nest registers each as its
/// own route). `None` for anything else — the caller treats that as "unresolvable, skip" (never guess).
// Bare `@Get` and empty-parens `@Get()` both yield one empty-string path.
fn decorator_paths(expr: &Expr) -> Option<Vec<String>> {
    let Expr::Call(call) = expr else {
        return Some(vec![String::new()]);
    };
    let Some(arg) = call.args.first() else {
        return Some(vec![String::new()]);
    };
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(vec![str_value(s)]),
        Expr::Array(ArrayLit { elems, .. }) => {
            let mut out = Vec::with_capacity(elems.len());
            for el in elems {
                let ExprOrSpread { spread: None, expr } = el.as_ref()? else {
                    return None; // a spread element — not a plain string-literal array
                };
                let Expr::Lit(Lit::Str(s)) = &**expr else {
                    return None; // a non-literal element — never guess the whole array
                };
                out.push(str_value(s));
            }
            Some(out)
        }
        _ => None, // dynamic path arg — never guess
    }
}

/// The decorator's callee/identifier name: `Get` from both bare `@Get` and called `@Get(...)`.
/// `None` for any unrecognized shape (a member expression, a non-identifier callee, ...).
fn decorator_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(id) => Some(id.sym.to_string()),
        Expr::Call(call) => match &call.callee {
            Callee::Expr(callee) => match &**callee {
                Expr::Ident(id) => Some(id.sym.to_string()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

fn method_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(str_value(s)),
        _ => None,
    }
}

fn prop_key_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(str_value(s)),
        _ => None,
    }
}

fn str_value(s: &Str) -> String {
    s.value.as_str().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    //! Coverage for `extract_controller_provides`: every prefix/path shape, the `@All` skip, the
    //! non-controller-class gate, and the dynamic-argument (never-guess) skips.
    use super::*;

    fn keys(out: &[IoProvide]) -> Vec<String> {
        out.iter().map(|p| p.key.clone()).collect()
    }

    #[test]
    fn bare_controller_and_bare_get_yield_a_root_route() {
        let src = "@Controller()\nclass C {\n  @Get()\n  ping() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /"]);
        assert_eq!(out[0].symbol.as_deref(), Some("ping"));
        assert_eq!(out[0].line, 3);
    }

    #[test]
    fn truly_bare_controller_with_no_parens_also_gates() {
        let src = "@Controller\nclass C {\n  @Get('x')\n  x() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /x"]);
    }

    #[test]
    fn controller_string_prefix_and_method_path_join() {
        let src = "@Controller('users')\nclass C {\n  @Get('active')\n  active() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /users/active"]);
    }

    #[test]
    fn controller_object_path_attribute_is_the_prefix() {
        let src = "@Controller({ path: 'users' })\nclass C {\n  @Get('active')\n  active() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /users/active"]);
    }

    #[test]
    fn controller_object_version_prefixes_a_v_segment() {
        let src = "@Controller({ path: 'users', version: '1' })\nclass C {\n  @Get('active')\n  active() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /v1/users/active"]);
    }

    #[test]
    fn every_method_decorator_maps_to_its_own_verb() {
        let src = concat!(
            "@Controller('items')\n",
            "class C {\n",
            "  @Get('a') a() {}\n",
            "  @Post('b') b() {}\n",
            "  @Put('c') c() {}\n",
            "  @Delete('d') d() {}\n",
            "  @Patch('e') e() {}\n",
            "}\n"
        );
        let out = extract_controller_provides("c.ts", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(
            got,
            vec![
                "DELETE /items/d",
                "GET /items/a",
                "PATCH /items/e",
                "POST /items/b",
                "PUT /items/c",
            ]
        );
    }

    #[test]
    fn path_param_is_normalized_by_http_interface_key() {
        let src = "@Controller('users')\nclass C {\n  @Get(':id')\n  x() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /users/{}"]);
    }

    #[test]
    fn array_of_paths_yields_one_provide_per_entry() {
        let src = "@Controller('items')\nclass C {\n  @Get(['a', 'b'])\n  x() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(got, vec!["GET /items/a", "GET /items/b"]);
    }

    #[test]
    fn all_decorator_is_skipped_not_guessed() {
        let src = "@Controller('items')\nclass C {\n  @All('x')\n  x() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert!(
            out.is_empty(),
            "@All must never guess-emit a verb, got: {out:?}"
        );
    }

    #[test]
    fn other_decorators_alongside_the_route_decorator_do_not_block_it() {
        let src =
            "@Controller('items')\nclass C {\n  @UseGuards(AuthGuard)\n  @Get('a')\n  a() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /items/a"]);
    }

    #[test]
    fn a_class_without_controller_emits_nothing() {
        let src = "class C {\n  @Get('a')\n  a() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert!(out.is_empty());
    }

    #[test]
    fn a_method_with_no_route_decorator_emits_nothing() {
        let src = "@Controller('items')\nclass C {\n  helper() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert!(out.is_empty());
    }

    #[test]
    fn dynamic_controller_prefix_skips_the_whole_controller() {
        let src = "@Controller(PREFIX)\nclass C {\n  @Get('a')\n  a() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert!(
            out.is_empty(),
            "a dynamic class prefix must never guess a path, got: {out:?}"
        );
    }

    // --- controller-prefix-ref-v1: dotted member-expression prefix (`@Controller(RouteKey.Asset)`) ---

    #[test]
    fn member_expression_controller_prefix_emits_fragments_not_provides() {
        let src = concat!(
            "@Controller(RouteKey.Asset)\n",
            "class AssetController {\n",
            "  @Get(':id')\n",
            "  getById() {}\n\n",
            "  @Delete()\n",
            "  remove() {}\n",
            "}\n"
        );
        // No direct provides — resolution is deferred to assemble time.
        let provides = extract_controller_provides("asset.controller.ts", src);
        assert!(
            provides.is_empty(),
            "a member-expression prefix must never guess a provide, got: {provides:?}"
        );

        let frags = extract_controller_prefix_route_fragments("asset.controller.ts", src);
        let mut got: Vec<(String, String, String, Option<String>)> = frags
            .iter()
            .map(|f| {
                (
                    f.prefix_ref.clone(),
                    f.verb.clone(),
                    f.path.clone(),
                    f.symbol.clone(),
                )
            })
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                (
                    "RouteKey.Asset".to_string(),
                    "DELETE".to_string(),
                    String::new(),
                    Some("remove".to_string())
                ),
                (
                    "RouteKey.Asset".to_string(),
                    "GET".to_string(),
                    ":id".to_string(),
                    Some("getById".to_string())
                ),
            ]
        );
        let get_by_id = frags
            .iter()
            .find(|f| f.symbol.as_deref() == Some("getById"));
        assert_eq!(get_by_id.unwrap().line, 3);
    }

    #[test]
    fn literal_controller_prefix_contributes_no_fragments() {
        let src = "@Controller('users')\nclass C {\n  @Get('active')\n  active() {}\n}\n";
        let frags = extract_controller_prefix_route_fragments("c.ts", src);
        assert!(frags.is_empty(), "{frags:?}");
        // And its provides are byte-identical to before this change.
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /users/active"]);
    }

    #[test]
    fn call_expression_prefix_still_skips_entirely() {
        let src = "@Controller(foo())\nclass C {\n  @Get('a')\n  a() {}\n}\n";
        assert!(extract_controller_provides("c.ts", src).is_empty());
        assert!(extract_controller_prefix_route_fragments("c.ts", src).is_empty());
    }

    #[test]
    fn deeper_dotted_chain_prefix_still_skips_the_whole_controller() {
        // `A.B.C` is not the exact two-segment shape `const_map_fragment` keys by — still a full skip,
        // not a fragment (module doc: "any OTHER non-literal shape ... still skips the whole controller").
        let src = "@Controller(RouteKey.Nested.Asset)\nclass C {\n  @Get('a')\n  a() {}\n}\n";
        assert!(extract_controller_provides("c.ts", src).is_empty());
        assert!(extract_controller_prefix_route_fragments("c.ts", src).is_empty());
    }

    #[test]
    fn dynamic_object_path_attribute_skips_the_whole_controller() {
        let src = "@Controller({ path: getPrefix() })\nclass C {\n  @Get('a')\n  a() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert!(out.is_empty());
    }

    #[test]
    fn dynamic_method_path_skips_only_that_method() {
        let src = "@Controller('items')\nclass C {\n  @Get(dynamicPath())\n  a() {}\n  @Get('b')\n  b() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /items/b"]);
    }

    #[test]
    fn dynamic_version_is_best_effort_skipped_not_the_whole_controller() {
        let src = "@Controller({ path: 'items', version: VERSION })\nclass C {\n  @Get('a')\n  a() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /items/a"]);
    }

    #[test]
    fn empty_file_yields_no_provides() {
        assert!(extract_controller_provides("e.ts", "").is_empty());
    }

    #[test]
    fn nest_shape_end_to_end() {
        let src = concat!(
            "import { Controller, Get, Post, Param, Body } from '@nestjs/common';\n\n",
            "@Controller('users')\n",
            "export class UsersController {\n",
            "  @Get()\n",
            "  findAll() {\n    return [];\n  }\n\n",
            "  @Get(':id')\n",
            "  findOne(@Param('id') id: string) {\n    return id;\n  }\n\n",
            "  @Post()\n",
            "  create(@Body() dto: unknown) {\n    return dto;\n  }\n",
            "}\n"
        );
        let out = extract_controller_provides("users.controller.ts", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(got, vec!["GET /users", "GET /users/{}", "POST /users"]);
        let find_one = out.iter().find(|p| p.symbol.as_deref() == Some("findOne"));
        assert_eq!(find_one.unwrap().key, "GET /users/{}");
    }

    #[test]
    fn rest_controller_gate_yields_provides_same_as_controller() {
        // `@n8n/decorators`'s `@RestController` is structurally identical to `@Controller`, under its own name.
        let src = "@RestController('/users')\nclass C {\n  @Get('/')\n  findAll() {}\n}\n";
        let out = extract_controller_provides("users.controller.ts", src);
        assert_eq!(keys(&out), vec!["GET /users"]);
        assert_eq!(out[0].symbol.as_deref(), Some("findAll"));
    }

    #[test]
    fn rest_controller_with_leading_slash_prefix_and_path() {
        // `@n8n/decorators` gives both the class prefix and the method path a leading slash, unlike
        // the no-leading-slash `@Controller('users')` convention — `http_interface_key`'s multi-slash
        // collapse must still produce a clean single-slash key.
        let src = concat!(
            "@RestController('/mfa')\n",
            "export class MFAController {\n",
            "  @Post('/enforce-mfa')\n",
            "  @GlobalScope('user:enforceMfa')\n",
            "  async enforceMFA() {}\n",
            "}\n"
        );
        let out = extract_controller_provides("mfa.controller.ts", src);
        assert_eq!(keys(&out), vec!["POST /mfa/enforce-mfa"]);
    }

    #[test]
    fn bare_rest_controller_with_no_parens_also_gates() {
        let src = "@RestController\nclass C {\n  @Get('/x')\n  x() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /x"]);
    }

    #[test]
    fn a_class_with_neither_controller_gate_still_emits_nothing() {
        // Regression guard: widening the gate set must not turn this into an unconditional pass.
        let src = "@Injectable()\nclass C {\n  @Get('a')\n  a() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert!(out.is_empty(), "{out:?}");
    }

    // -- extract_controller_guarded_lines --

    #[test]
    fn class_level_use_guards_covers_every_route_in_the_controller() {
        let src = concat!(
            "@Controller('items')\n",
            "@UseGuards(JwtAuthGuard)\n",
            "class C {\n",
            "  @Get('a')\n",
            "  a() {}\n\n",
            "  @Post('b')\n",
            "  b() {}\n",
            "}\n"
        );
        let out = extract_controller_provides("c.ts", src);
        let a_line = out
            .iter()
            .find(|p| p.symbol.as_deref() == Some("a"))
            .unwrap()
            .line;
        let b_line = out
            .iter()
            .find(|p| p.symbol.as_deref() == Some("b"))
            .unwrap()
            .line;
        let guarded = extract_controller_guarded_lines("c.ts", src);
        assert!(guarded.contains(&a_line), "{guarded:?}");
        assert!(guarded.contains(&b_line), "{guarded:?}");
    }

    #[test]
    fn method_level_use_guards_covers_only_that_route() {
        let src = concat!(
            "@Controller('items')\n",
            "class C {\n",
            "  @UseGuards(JwtAuthGuard)\n",
            "  @Get('a')\n",
            "  a() {}\n\n",
            "  @Post('b')\n",
            "  b() {}\n",
            "}\n"
        );
        let out = extract_controller_provides("c.ts", src);
        let a_line = out
            .iter()
            .find(|p| p.symbol.as_deref() == Some("a"))
            .unwrap()
            .line;
        let b_line = out
            .iter()
            .find(|p| p.symbol.as_deref() == Some("b"))
            .unwrap()
            .line;
        let guarded = extract_controller_guarded_lines("c.ts", src);
        assert!(guarded.contains(&a_line), "{guarded:?}");
        assert!(
            !guarded.contains(&b_line),
            "sibling unguarded route must not be in the guarded set: {guarded:?}"
        );
    }

    #[test]
    fn no_use_guards_anywhere_yields_an_empty_set() {
        let src = "@Controller('items')\nclass C {\n  @Get('a')\n  a() {}\n}\n";
        let guarded = extract_controller_guarded_lines("c.ts", src);
        assert!(guarded.is_empty(), "{guarded:?}");
    }

    #[test]
    fn a_non_controller_class_with_use_guards_yields_an_empty_set() {
        let src = "class C {\n  @UseGuards(JwtAuthGuard)\n  @Get('a')\n  a() {}\n}\n";
        let guarded = extract_controller_guarded_lines("c.ts", src);
        assert!(guarded.is_empty(), "{guarded:?}");
    }

    #[test]
    fn class_level_guards_cover_a_wildcard_post_route() {
        // Covers a wildcard POST handler whose own body never calls anything guard-named.
        let src = concat!(
            "@Controller('rest')\n",
            "@UseGuards(JwtAuthGuard, WorkspaceAuthGuard)\n",
            "export class RestApiCoreController {\n",
            "  @Post('*path')\n",
            "  async handleApiPost() {}\n",
            "}\n"
        );
        let out = extract_controller_provides("rest.controller.ts", src);
        let post_line = out
            .iter()
            .find(|p| p.symbol.as_deref() == Some("handleApiPost"))
            .unwrap()
            .line;
        let guarded = extract_controller_guarded_lines("rest.controller.ts", src);
        assert!(guarded.contains(&post_line), "{guarded:?}");
    }

    #[test]
    fn rest_controller_gate_participates_in_guarded_lines_the_same_as_controller() {
        let src = "@RestController('/items')\n@UseGuards(JwtAuthGuard)\nclass C {\n  @Post('/a')\n  a() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        let a_line = out[0].line;
        let guarded = extract_controller_guarded_lines("c.ts", src);
        assert!(guarded.contains(&a_line), "{guarded:?}");
    }

    #[test]
    fn global_scope_and_licensed_decorators_are_not_recognized_as_use_guards() {
        // Deliberate non-widening (module doc "Known residual"): only a literal `@UseGuards` counts.
        let src = concat!(
            "@RestController('/users')\n",
            "class C {\n",
            "  @Post('/:id/role')\n",
            "  @GlobalScope('user:changeRole')\n",
            "  @Licensed('feat:advancedPermissions')\n",
            "  changeRole() {}\n",
            "}\n"
        );
        let out = extract_controller_provides("users.controller.ts", src);
        let route_line = out[0].line;
        let guarded = extract_controller_guarded_lines("users.controller.ts", src);
        assert!(
            !guarded.contains(&route_line),
            "GlobalScope/Licensed must not be treated as UseGuards coverage: {guarded:?}"
        );
    }

    // -- body-shape-v1: `@Body()` request-body contract capture --

    fn body_of<'a>(out: &'a [IoProvide], symbol: &str) -> Option<&'a ProvideBodyShape> {
        out.iter()
            .find(|p| p.symbol.as_deref() == Some(symbol))
            .and_then(|p| p.body.as_ref())
    }

    #[test]
    fn body_decorator_with_string_sub_key_and_dto_type_is_captured() {
        let src = concat!(
            "@Controller('users')\n",
            "class C {\n",
            "  @Post()\n",
            "  create(@Body('user') u: CreateUserDto) {}\n",
            "}\n"
        );
        let out = extract_controller_provides("c.ts", src);
        let body = body_of(&out, "create").expect("body must be captured");
        assert_eq!(body.sub_key.as_deref(), Some("user"));
        assert_eq!(body.dto_ref.as_deref(), Some("CreateUserDto"));
        assert!(body.fields.is_empty());
        assert!(!body.complete);
    }

    #[test]
    fn bare_body_decorator_yields_whole_body_sub_key_none() {
        let src = concat!(
            "@Controller('users')\n",
            "class C {\n",
            "  @Post()\n",
            "  create(@Body() dto: CreateUserDto) {}\n",
            "}\n"
        );
        let out = extract_controller_provides("c.ts", src);
        let body = body_of(&out, "create").expect("body must be captured");
        assert_eq!(body.sub_key, None);
        assert_eq!(body.dto_ref.as_deref(), Some("CreateUserDto"));
    }

    #[test]
    fn body_decorator_with_no_type_annotation_yields_no_body() {
        let src = concat!(
            "@Controller('users')\n",
            "class C {\n",
            "  @Post()\n",
            "  create(@Body() dto) {}\n",
            "}\n"
        );
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(body_of(&out, "create"), None);
    }

    #[test]
    fn two_body_decorators_on_one_method_yield_no_body() {
        let src = concat!(
            "@Controller('users')\n",
            "class C {\n",
            "  @Post()\n",
            "  create(@Body('a') a: A, @Body('b') b: B) {}\n",
            "}\n"
        );
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(body_of(&out, "create"), None);
    }

    #[test]
    fn non_literal_body_decorator_argument_yields_no_body() {
        let src = concat!(
            "@Controller('users')\n",
            "class C {\n",
            "  @Post()\n",
            "  create(@Body(v) dto: CreateUserDto) {}\n",
            "}\n"
        );
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(body_of(&out, "create"), None);
    }

    #[test]
    fn primitive_type_annotation_yields_no_body() {
        let src = concat!(
            "@Controller('users')\n",
            "class C {\n",
            "  @Post()\n",
            "  create(@Body('email') e: string) {}\n",
            "}\n"
        );
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(body_of(&out, "create"), None);
    }

    #[test]
    fn non_body_routes_keep_body_none() {
        let src = "@Controller('items')\nclass C {\n  @Get('a')\n  a() {}\n}\n";
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(body_of(&out, "a"), None);
    }

    #[test]
    fn prefix_ref_fragment_carries_body_too() {
        let src = concat!(
            "@Controller(RouteKey.Asset)\n",
            "class AssetController {\n",
            "  @Post()\n",
            "  create(@Body('user') u: CreateUserDto) {}\n",
            "}\n"
        );
        let frags = extract_controller_prefix_route_fragments("asset.controller.ts", src);
        let create = frags
            .iter()
            .find(|f| f.symbol.as_deref() == Some("create"))
            .expect("fragment must be emitted");
        let body = create.body.as_ref().expect("body must be captured");
        assert_eq!(body.sub_key.as_deref(), Some("user"));
        assert_eq!(body.dto_ref.as_deref(), Some("CreateUserDto"));
    }
}
