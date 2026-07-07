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
//!   "never guess" IO convention — see `egress.rs`'s `resolve_url`).
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
    ObjectLit, Prop, PropName, PropOrSpread, Str,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_interface_key, IoProvide};

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
        if let Some(ctx) = controller_context(&n.class.decorators) {
            for member in &n.class.body {
                if let ClassMember::Method(m) = member {
                    self.emit_method(&ctx.prefix, m);
                }
            }
        }
        n.visit_children_with(self); // recurse — covers any nested class declarations
    }
}

impl ControllerCollector<'_> {
    fn emit_method(&mut self, prefix: &str, m: &ClassMethod) {
        let Some(name) = method_name(&m.key) else {
            return; // computed/private method key — not a recognizable route handler name
        };
        let Some((verb, decorator, paths)) = method_route(&m.function.decorators) else {
            return; // no recognized route decorator, `@All`, or an unresolvable (dynamic) path
        };
        let line = crate::line_of(self.cm, decorator.span.lo);
        for path in paths {
            let full_path = format!("{prefix}/{path}");
            self.out.push(IoProvide {
                kind: "http".to_string(),
                key: http_interface_key(&verb, &full_path),
                file: self.file.to_string(),
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

/// A controller class's own routing context: the resolved path prefix (already includes the
/// `v<version>` segment when `version` was a string literal — see module doc).
struct ControllerCtx {
    prefix: String,
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
        return Some(ControllerCtx {
            prefix: String::new(),
        });
    };
    let Some(arg) = call.args.first() else {
        return Some(ControllerCtx {
            prefix: String::new(),
        });
    };
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(ControllerCtx {
            prefix: str_value(s),
        }),
        Expr::Object(obj) => object_controller_ctx(obj),
        _ => None, // dynamic prefix arg — never guess, skip the whole controller (see module doc)
    }
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
    Some(ControllerCtx { prefix })
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
}
