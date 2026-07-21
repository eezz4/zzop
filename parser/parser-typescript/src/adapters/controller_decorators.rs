//! Controller-decorator-shape HTTP route PROVIDES extraction — swc-AST-based (mirrors
//! `zzop_parser_java_21::provides`'s Spring extractor: class-level prefix + method-level verb/path,
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
//!   `zzop_parser_java_21::provides`'s ambiguous bare `@RequestMapping` skip.
//! - Every decorator on a method is scanned for a route-verb name (or `@All`), not just the first,
//!   so other decorators (`@UseGuards`, `@ApiTags`, ...) and decorator order never matter.
//!
//! ## Known limits (v1 scope, not fixed)
//! - Lexical name matching only — import source is never verified, so a same-named decorator from an
//!   unrelated library plus a same-named class gate would false-positive (same tradeoff as the Java
//!   annotation extractor; the required double collision makes it vanishingly rare).
//! - Method-level `@Version()` overrides are not read.
//! - An array `path` prefix (`{ path: ['a','b'] }`) takes only the first literal entry, same
//!   "first wins" simplification as `zzop_parser_java_21::provides`'s `first_quoted_string`.
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

mod context;
mod method_facts;
mod provides;

pub use provides::{
    extract_controller_guarded_lines, extract_controller_prefix_route_fragments,
    extract_controller_provides,
};

#[cfg(test)]
mod tests;
