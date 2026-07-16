//! Producer FRAGMENT shapes — per-file projections the engine composes into whole-tree `IoProvide`s
//! (`zzop_engine::analyze`'s `compose_trpc_provides` / `compose_router_mount_provides`). These types
//! live in `zzop-core`, not the parser crate that produces them: a parser crate produces one file's
//! fragment, the engine composes fragments tree-wide, and `zzop-cache` round-trips a fragment
//! verbatim through its on-disk `FileIrSlice` — three crates need one concrete type.
//!
//! tRPC routers (`ProcedureRouterEntry`/`ProcedureRouterFragment`) compose across files: a router typically
//! imports and re-mounts a sub-router under a key, so a nested leaf's full route path is only
//! knowable once every file's fragment is assembled. Router-mount fragments
//! (`RouterMountEntry`/`RouterMountFragment`) are the same idea for a code-registered router split
//! across files (verb registrations on sub-routers, sub-routers mounted with a prefix, the app
//! mounted again). Each file reports only what its own text says; the engine's assembly pass
//! resolves across files and emits `IoProvide`s. The fragment shape is framework-agnostic — only the
//! producer's recognizer varies; see `zzop_parser_typescript::adapters::router_mounts` for the
//! current one.
//!
//! Router-mount fragments can additionally carry PRODUCER-JUDGED attributes (`RouterMountEntry::Verb::attr_keys`,
//! `RouterMountEntry::Mount::attr_keys`, `RouterMountEntry::ScopedAttr`) — open-vocabulary facts (e.g.
//! "auth-guarded") composed at assemble time into `zzop_core::Attribute`s on the same `zzop_core::AttributeStore`
//! channel a Mode-B overlay feeds. The kernel never interprets these keys; see `zzop_core::attributes`'s
//! module doc for the channel itself.

use serde::{Deserialize, Serialize};

/// One entry of a tRPC router's object literal (or a `mergeRouters` argument list).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProcedureRouterEntry {
    /// A procedure leaf: `verb` is `"QUERY"` | `"MUTATION"` | `"SUBSCRIPTION"`; `line` anchors the emitted `IoProvide`.
    Leaf {
        key: String,
        verb: String,
        line: u32,
    },
    /// A sub-router reference by identifier; `specifier` is set when `ident` is imported, `None` for
    /// a same-file binding. A `mergeRouters(...)` argument is a `Ref` with `key: String::new()`.
    Ref {
        key: String,
        ident: String,
        specifier: Option<String>,
    },
    /// An inline nested `router({...})` (or `createTRPCRouter({...})`) call as a property value.
    Nested {
        key: String,
        entries: Vec<ProcedureRouterEntry>,
    },
}

/// One `const <name> = router({...})` top-level binding in a file. A cross-file `Ref` only resolves
/// if the binding is exported — checked by the assembling engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureRouterFragment {
    pub name: String,
    pub entries: Vec<ProcedureRouterEntry>,
}

/// One entry of a router-mount fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouterMountEntry {
    /// A concrete verb registration, e.g. `recv.post('/setup', handler)`. `method` is UPPERCASE;
    /// `path` is the verbatim string-literal first argument (`:param` normalized later by
    /// `http_interface_key`); `handler` is the last argument's name; `line` is 1-based.
    Verb {
        method: String,
        path: String,
        handler: Option<String>,
        line: u32,
        /// Attribute keys the producer attaches to this route's composed IoKey (open vocabulary — the
        /// kernel never interprets them; the value is implicitly `true`). E.g. "auth-guarded" from a
        /// route-level guard argument.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attr_keys: Vec<String>,
    },
    /// A sub-router mount, e.g. `recv.route('/two-factor', twoFactorRoute)` (Hono): `prefix` is the
    /// first argument; `ident`/`specifier` identify the mounted router (`None` = local).
    Mount {
        prefix: String,
        ident: String,
        specifier: Option<String>,
        /// Producer-judged attribute keys for the case this mount does NOT resolve to a router fragment:
        /// a `.use(prefix, ident)` cannot be locally disambiguated between "sub-router" and "middleware
        /// guard" — the composer resolves it. Resolved: a normal mount, keys ignored (the ident was a
        /// router). Unresolved: each key becomes a PathScope attribute at the composed prefix.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attr_keys: Vec<String>,
    },
    /// A producer-judged cross-cutting attribute scoped to this router's local `prefix` — e.g. an
    /// Express `app.use('/admin', requireAuth())` middleware guard. Resolved to a final `PathScope`
    /// attribute once the router's mount chain is composed; the value is implicitly `true` (a
    /// `serde_json::Value` here would break the `Eq` derive, and every current producer emits
    /// presence facts — generalize when a non-presence native producer exists).
    ScopedAttr {
        /// Router-local literal path prefix the attribute covers ("/" = the whole router).
        prefix: String,
        /// Open-vocabulary attribute key (producer-owned, e.g. "auth-guarded").
        key: String,
        /// 1-based line of the registration call.
        line: u32,
    },
}

/// All entries registered on one router identifier within one file. `name` is the identifier the
/// router is bound to (`auth`, `twoFactorRoute`, ...; `"default"` for `export default new Hono()`
/// chains with no binding).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouterMountFragment {
    pub name: String,
    pub entries: Vec<RouterMountEntry>,
}

/// ## Wrapper-call fragments (`WrapperDefFragment` / `WrapperCallFragment`)
/// A frontend codebase frequently wraps its actual HTTP-call sink (`fetch`/`axios`/`ky`) behind a
/// project-local helper function that forwards to a shared request/axios call. Without this join,
/// every call site's HTTP consume gets anchored to the wrapper's own definition site instead of the
/// real call site.
///
/// `WrapperDefFragment` records a qualifying exported function's call signature (which parameter
/// carries the verb, which the path) indexed by `(file, name)`. `WrapperCallFragment` records every
/// plausible call site's literal arguments. The engine's assemble-time join resolves each call back
/// to a def fragment and emits the `http` consume at the real call site — precision comes from the
/// def side's signature gate; the call side is deliberately permissive since the join filters real
/// invocations out of it. See `zzop_parser_typescript::adapters::wrapper_calls` for the recognizer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrapperDefFragment {
    /// The exported function name.
    pub name: String,
    /// 0-based parameter index carrying the HTTP verb; `None` when hardcoded (see `fixed_method`).
    pub method_param: Option<u32>,
    /// 0-based parameter index carrying the URL path.
    pub path_param: u32,
    /// The UPPERCASE verb when hardcoded, mutually exclusive with `method_param` in practice.
    pub fixed_method: Option<String>,
}

/// One plausible wrapper call site — see `WrapperDefFragment`'s doc for the assemble-time join.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrapperCallFragment {
    /// The plain identifier called (e.g. `makeRestApiRequest`).
    pub callee: String,
    /// The import specifier when `callee` is imported in this file; `None` for a same-file local.
    pub specifier: Option<String>,
    /// The first 6 call arguments, positionally: a string literal's verbatim text; a template
    /// literal's shape with `${...}` replaced by `{}`; `None` for any other argument shape.
    pub args: Vec<Option<String>>,
    /// 1-based source line of the call expression.
    pub line: u32,
}

/// One NestJS-shaped controller route whose CLASS-LEVEL prefix is a dotted member-expression reference
/// (`@Controller(RouteKey.Asset)`) rather than a string literal — emitted by
/// `zzop_parser_typescript::adapters::controller_decorators` INSTEAD OF a direct `IoProvide` when a
/// controller's prefix arg is exactly the `Ident.Ident` shape `zzop_parser_typescript::const_map_fragment`
/// keys its constant-map entries by (a single-file scan cannot know whether `RouteKey.Asset` resolves —
/// the `enum`/`const` declaring it commonly lives in another file).
///
/// Resolved at assemble time (`zzop_engine::analyze::compose`'s controller-prefix composer) against the
/// SAME project-wide merged const map the late cross-file CONSUME re-resolution uses (itself now also
/// folding string-valued `enum` members — see `const_map_fragment`'s doc). A `prefix_ref` absent from
/// that merged map is warned and its routes are dropped — never guessed, never emitted unprefixed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerPrefixRouteFragment {
    /// The decorator argument's verbatim dotted text, e.g. `"RouteKey.Asset"` — looked up in the merged
    /// const map at assemble time.
    pub prefix_ref: String,
    /// UPPERCASE HTTP verb (`"GET"`, `"POST"`, ...) — same vocabulary as `IoProvide::key`'s verb segment.
    pub verb: String,
    /// The method-level route path, possibly empty (a bare `@Get()` decorator) — joined onto the
    /// resolved prefix as `"{prefix}/{path}"`, mirroring `extract_controller_provides`'s own join.
    pub path: String,
    /// 1-based source line of the route decorator — anchors the composed `IoProvide`.
    pub line: u32,
    /// The route handler method's name.
    pub symbol: Option<String>,
    /// The handler's `@Body()` request-body contract (`body-shape-v1`), carried through so a
    /// prefix-ref route's composed `IoProvide` keeps the same body evidence a literal-prefix
    /// route gets directly — `#[serde(default)]` so pre-existing serialized fragments (and
    /// producers that don't capture bodies) deserialize as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<crate::io::ProvideBodyShape>,
}

/// The field shape of one class declaration — the per-file half of request-body DTO resolution
/// (`IoProvide::body`'s `dto_ref` is looked up against the tree-wide merge of these at assemble
/// time, mirroring the const-map/controller-prefix pattern: a single-file scan cannot know where
/// `CreateUserDto` is declared). Emitted for EVERY class declaration, field-less ones included —
/// a field-less `extends PartialType(X) {}` resolves as "found but incomplete", a more informative
/// signal than "not found"; classes are cheap to carry (name + field names + flags).
///
/// Never-guess at assemble: a `dto_ref` missing from the merged map, or a class name declared
/// with CONFLICTING shapes in two files, resolves to nothing (the provide keeps `body: None`
/// semantics by dropping the shape) — one aggregated warning, no guessed fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassShapeFragment {
    /// The class's declared name (`class CreateUserDto` → `"CreateUserDto"`).
    pub name: String,
    /// Property members with statically known names (`PropName::Ident`/`Str`), in source order.
    pub fields: Vec<crate::io::ProvideBodyField>,
    /// `false` when the field list may be partial: an `extends` clause, constructor parameter
    /// properties, an index signature, or a computed property key.
    pub complete: bool,
}
