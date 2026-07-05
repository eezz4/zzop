//! Producer FRAGMENT shapes — per-file projections the engine composes into whole-tree `IoProvide`s
//! (`zzop_engine::analyze`'s `compose_trpc_provides` / `compose_router_mount_provides`). These types
//! live in `zzop-core`, not the parser crate that produces them: a parser crate produces one file's
//! fragment, the engine composes fragments tree-wide, and `zzop-cache` round-trips a fragment
//! verbatim through its on-disk `FileIrSlice` — three crates need one concrete type.
//!
//! tRPC routers (`TrpcRouterEntry`/`TrpcRouterFragment`) compose across files: a router typically
//! imports and re-mounts a sub-router under a key, so a nested leaf's full route path is only
//! knowable once every file's fragment is assembled. Router-mount fragments
//! (`RouterMountEntry`/`RouterMountFragment`) are the same idea for a code-registered router split
//! across files (verb registrations on sub-routers, sub-routers mounted with a prefix, the app
//! mounted again). Each file reports only what its own text says; the engine's assembly pass
//! resolves across files and emits `IoProvide`s. The fragment shape is framework-agnostic — only the
//! producer's recognizer varies; see `zzop_parser_typescript::adapters::router_mounts` for the
//! current one.

use serde::{Deserialize, Serialize};

/// One entry of a tRPC router's object literal (or a `mergeRouters` argument list).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TrpcRouterEntry {
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
        entries: Vec<TrpcRouterEntry>,
    },
}

/// One `const <name> = router({...})` top-level binding in a file. A cross-file `Ref` only resolves
/// if the binding is exported — checked by the assembling engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrpcRouterFragment {
    pub name: String,
    pub entries: Vec<TrpcRouterEntry>,
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
    },
    /// A sub-router mount, e.g. `recv.route('/two-factor', twoFactorRoute)` (Hono): `prefix` is the
    /// first argument; `ident`/`specifier` identify the mounted router (`None` = local).
    Mount {
        prefix: String,
        ident: String,
        specifier: Option<String>,
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
