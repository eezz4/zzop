//! zzop-parser-typescript — native swc TS parser -> Common IR projection (0 N-API crossings). swc types
//! stay inside this crate (an swc upgrade should never leak into the public IR); only zzop-core types are
//! exposed.
//!
//! ## 2-layer layout
//! - `lang` — swc -> Common-IR LANGUAGE projection: call-graph construction (`calls`) and dependency-path
//!   resolution (`resolve`). Symbol/import extraction lives in sibling crate-root modules since both `lang` and `adapters`
//!   depend on it.
//! - `adapters` — framework-vocabulary producers emitting `IoConsume`/`IoProvide`/fragment IR (controller
//!   decorators, FE HTTP-call egress, tRPC routers/proxy clients, Next.js `pages/api` handlers,
//!   Hono-style router mounts).

pub mod adapters;
mod cjs_exports;
mod cjs_require;
mod factory;
mod ident_refs;
mod imports;
pub mod lang;
mod loop_spans;
mod parse;
mod project;
mod re_exports;
mod symbol_shapes;
mod symbols;
#[cfg(test)]
mod symbols_tests;
#[cfg(test)]
mod test_util;

pub use adapters::class_shapes::extract_class_shape_fragments;
pub use adapters::client_base::{extract_client_base_prefix_marker, CLIENT_BASE_PREFIX_KIND};
pub use adapters::controller_decorators::{
    extract_controller_guarded_lines, extract_controller_prefix_route_fragments,
    extract_controller_provides,
};
pub use adapters::db_table_consume::{
    extract_db_table_consumes, extract_query_call_sites, PRISMA_CLIENT_GETTER,
};
pub use adapters::egress::{
    base_relative_path, const_map_fragment, extract_http_egress, is_external_url, resolve_raw_path,
};
pub use adapters::global_prefix::{extract_global_prefix_marker, NEST_GLOBAL_PREFIX_KIND};
pub use adapters::hono_client::extract_hono_client_consumes;
pub use adapters::next_pages_api::{scan_pages_api_handler, PagesApiHandlerScan};
pub use adapters::pathname_dispatch::{
    extract_pathname_dispatch_provides, PATHNAME_DISPATCH_FALLBACK_VERBS,
};
pub use adapters::router_mounts::extract_router_mount_fragments;
pub use adapters::trpc_consume::extract_trpc_consumes;
pub use adapters::trpc_router::extract_procedure_router_fragments;
pub use adapters::wrapper_calls::extract_wrapper_fragments;
pub use lang::calls::parse_calls;
pub use lang::resolve::{
    build_dep, build_dep_with_workspace, resolve_file, resolve_file_with_workspace, try_ext,
    TsconfigPaths, WorkspacePkg, RESOLVE_EXTS,
};
pub use lang::write_site::{
    write_sites_for_symbol, DEFAULT_ORM_RECEIVER_PATTERN, DEFAULT_WRITE_METHODS,
};

pub use ident_refs::parse_local_identifier_refs;
pub use imports::parse_imports;
pub use loop_spans::extract_loop_spans;
pub use parse::parse_ok;
pub(crate) use parse::{line_of, parse_module, parse_with_cm};
pub use project::{build_common_ir, count_loc};
pub use re_exports::{parse_dynamic_imports, parse_re_exports};
pub use symbols::parse_symbols;

/// Cache key ingredient for `zzop-cache`: parser id + pinned swc version + a logic-version counter, so an
/// swc upgrade or a change in this crate's projected IR shape invalidates stale cached entries. The
/// `swc_core-71.0.5` segment must match this crate's `Cargo.toml` pin exactly (TODO Phase 2: derive it
/// from the pin automatically instead of hand-syncing). Each `+name-vN` suffix marks a projection-shape
/// change — new IO kind, new fragment type, or a changed field on an existing one — that a cache entry
/// from before that marker would not reflect and must not be served as fresh:
/// - `v3` -> `v4`: NestJS-style `@Controller`/`@Get`/... route PROVIDES extraction.
/// - `late-resolve-v1`: `IoConsume::method` now set on unresolved consumes; added the late cross-file
///   constant re-resolution substrate (`const_map_fragment`/`resolve_raw_path`).
/// - `oazapfts-v1`: recognizes the oazapfts-generated-SDK call family in HTTP egress. RETIRED by
///   `oazapfts-removed-v1` below (decision: generated SDKs are injection adapters, not engine vocab) —
///   left here so the marker's history stays readable; the recognition itself no longer exists.
/// - `trpc-v1`: tRPC consume extraction plus per-file tRPC router fragments.
/// - `router-mounts-v1`: code-registered router-mount fragments (Hono-style), replacing the old
///   line-based route extractor — sees chained builders and cross-file mounts it couldn't before.
/// - `wrapper-calls-v1`: FE HTTP-call wrapper fragments, re-anchoring consumes from a wrapper's
///   internals to its real cross-file call sites.
/// - `hono-client-v1`: Hono's typed `hc<AppType>()` proxy-client call shape as HTTP consumes.
/// - `router-mounts-v2`: router-mount fragments gain an Express vocabulary alongside Hono; the
///   fragment shape and engine-side compose pass are unchanged, only the recognizer's vocabulary grew.
/// - `query-call-sites-v1`: `extract_query_call_sites` — per-file `zzop_core::QueryCallSite` facts for
///   the schema x usage JOIN rules, replacing `zzop_rules_schema::join`'s own filesystem re-walk.
/// - `store-binding-removed-v1`: the per-file store-binding recognizer (`extract_store_bound_models`) was
///   removed — it recognized one project's `createStore`/`STORES`/`/domains/` convention, an app-specific
///   environment that belongs in a Mode-B overlay, not native. `dead-model` now keys on the generic
///   `identifier_counts` presence signal (see `zzop_rules_schema::usage`); a store binding is now injected
///   as a generic `bound-model` attribute on the model `Symbol` (the entity-attribute channel), not a slot.
/// - `write-sites-v1`: `SourceSymbol::write_sites` — per-symbol store-write site detection, computed once
///   here instead of `zzop_rules_http::http_scan` re-scanning each BFS-reached symbol's raw text on every
///   analysis run.
/// - `reexport-edges-v1`: `FileArtifact`/`FileIrSlice` now carry each file's `parse_re_exports` output
///   (specifier + `type_only`) so `lang::resolve::build_dep`/`build_dep_with_workspace` can merge
///   non-type-only re-export specifiers into the dep graph as real edges — a barrel file's re-exports
///   used to be invisible to `dep`, undercounting fan-in and false-positiving `dead-candidates`.
/// - `dynamic-import-edges-v1`: `FileArtifact`/`FileIrSlice` now carry each file's `parse_dynamic_imports`
///   output (dynamic `import()` specifiers), merged into the dep graph as real edges (excluded from
///   circular) so a code-split-only module keeps its fan-in and isn't false-positived by
///   `dead-candidates`. Also, a type-only re-export now gains the same edge-but-excluded-from-cycles
///   treatment a type-only import binding already had, instead of being dropped entirely.
/// - `nest-global-prefix-v1`: `extract_global_prefix_marker` — a NestJS `app.setGlobalPrefix('api')`
///   sentinel `IoProvide { kind: "nest-global-prefix", ... }`, ridden on the existing `provides` channel
///   (no cache-schema bump) so `zzop-engine`'s tree assembly can prepend the global prefix onto every
///   `http` provide key and then strip the sentinel before output.
/// - `base-relative-egress-v1`: a base-relative path literal on a recognized HTTP call
///   (`axios.get('users/login')` — the `baseURL` idiom) now keys as its root-normalized path
///   (`GET /users/login`) instead of falling unresolved; see `adapters::egress::base_relative_path`'s
///   veto list for what still never keys.
/// - `query-drop-v1`: HTTP CONSUME keys drop any `?...`/`#...` query/fragment suffix
///   (`core::http_consume_interface_key`) — `axios.get('articles?limit=10')` and
///   `` axios.get(`articles?${qs}`) `` now key as `GET /articles`, so they can exact-join (or
///   route-near-miss against) the provide, instead of being structurally unmatchable. Provide keys
///   are untouched (`?` in a route PATTERN is not a query separator).
/// - `controller-prefix-ref-v1`: `const_map_fragment` now also folds every top-level (incl. `export`)
///   `enum`'s string-valued members (`RouteKey.Asset -> "assets"`); AND a `@Controller(RouteKey.Asset)`
///   dotted member-expression prefix no longer skips the whole controller — its methods are now
///   projected as `zzop_core::ControllerPrefixRouteFragment`s
///   (`extract_controller_prefix_route_fragments`), resolved against the merged const map at assemble
///   time instead of being dropped outright.
/// - `angular-httpclient-v1`: HTTP egress now recognizes Angular's dependency-injected `HttpClient`
///   call shape (`this.<name>.get/post/put/delete/patch(url)` / `<name>.get/...(url)`), gated per-file
///   on an `@angular/common/http` import plus a proven HttpClient receiver (constructor param property,
///   class property, or `inject(HttpClient)`) — see `adapters::egress` module doc.
/// - `loop-spans-v1`: `extract_loop_spans` — per-file loop-body line spans (`zzop_core::dsl::
///   SourceFile::loop_spans`), feeding `MethodScan::trigger_in_loop`: every `for`/`for-in`/`for-of`
///   (incl. `for await`)/`while`/`do-while` statement's whole span, plus the callback-argument-only span
///   of a recognized array-iteration call (see [`ARRAY_ITERATION_METHODS`]).
/// - `pathname-dispatch-v1`: `extract_pathname_dispatch_provides` — manual pathname-dispatch route
///   PROVIDES (`if (url.pathname === "/x")` chains / `switch (url.pathname)`) from framework-less
///   servers (raw Cloudflare Workers, Node `http.createServer`, ...), evidence-gated on URL
///   provenance plus a Request-typed/named parameter in the same function, with Durable-Object
///   class bodies vetoed — see `adapters::pathname_dispatch` module doc.
/// - `base-carrier-drop-v1`: a consume URL variant with exactly one leading dynamic piece followed
///   by a `/`-headed literal (`` `${BASE_URL}/me/x` ``, `BASE + '/x'`) now keys as its visible
///   path (`GET /me/x`) instead of falling unresolved — the opaque base is dropped, never valued.
///   `{}{}`-heads, non-`/` suffixes (invisible segment boundary), and post-drop `//` (host
///   carrier) still refuse to key — see `adapters::egress`'s `consume_key_for`.
/// - `body-shape-v1`: HTTP consume sites additionally carry the statically witnessed request-body
///   object-literal key shape (`IoConsume::body`), controller `@Body()` params carry their DTO
///   type ref (`IoProvide::body`), and every class declaration's field shape is emitted as a
///   `ClassShapeFragment` for assemble-time DTO resolution — `cross-layer/body-field-drift`'s
///   substrate. See `adapters::class_shapes` / `adapters::egress` / `adapters::controller_decorators`.
/// - `axios-defaults-base-v1`: egress consumes carry a `client` provenance tag, and a literal
///   `axios.defaults.baseURL = "..."` assignment emits a sentinel consume whose PATH PART the
///   engine prepends to that tree's axios-tagged consume keys at assemble time (host deliberately
///   ignored — deploy config, not contract; same effective-URL stance as the openapi adapter's
///   `servers[].url` handling). Non-literal base values stay uninterpreted (adapter overlays cover
///   them) — see `adapters::client_base`.
/// - `oazapfts-removed-v1`: oazapfts recognition removed from HTTP egress (the `oazapfts-v1` call
///   family, its `method`/`body` wrapper-unwrap reads, and the trailing `QS.`-suffix template drop) —
///   generated SDKs are injection adapters, not engine vocab; the vocabulary moves to
///   `examples/oazapfts-adapter`. Extraction output changes (fewer consumes recognized, a trailing
///   `QS.` interpolation now keys as an ordinary `{}` placeholder instead of being dropped), so cached
///   entries from before this marker must not be served as fresh.
/// - `express-middleware-v1`: `router_mounts` now judges common Express middleware guard
///   registrations (`app.use(prefix, requireAuth())`, a route-level middle argument, a
///   `.use(prefix, mw1, mw2)` list) against a narrow guard-name/callee vocabulary (the
///   `is_guard_name` predicate / `MIDDLEWARE_GUARD_CALLEES`, vetoed first by
///   `ROUTER_NAME_VETO_SUFFIXES`), emitting the judgment as `RouterMountEntry::Verb::attr_keys` /
///   `RouterMountEntry::Mount::attr_keys` / the new `RouterMountEntry::ScopedAttr` variant — composed
///   at assemble time into the generic entity-attribute channel (`zzop_core::AttributeStore`) that
///   `zzop_rules_http::mutating_route_no_auth` already consumed for Mode-B overlay evidence. Fragment
///   shape change (new fields + variant), so cached entries from before this marker must not be
///   served as fresh.
/// - `db-table-bare-receiver-v1`: `db_table_consume`'s recognizer now ALSO anchors on a bare
///   `<receiver>.<model>.<method>(...)` call (both `extract_db_table_consumes` and
///   `extract_query_call_sites`), where `<receiver>` is a plain identifier this file has import
///   evidence binds to a Prisma client (`prisma_bound_receivers`) — the be-express
///   `import prisma from '../prisma/prisma-client'` idiom, previously invisible (`getPrisma()`-only).
///   Strictly additive (more call shapes recognized, no existing shape's output changes), but cached
///   entries from before this marker must not be served as fresh since they lack the new facts.
/// - `intra-file-wrapper-v1`: the wrapper-def recognizer now ALSO collects file-PRIVATE top-level
///   wrapper functions/const-arrows (a `Stmt::Decl`, not just `export`ed decls) — the common idiom of
///   a private `function request(method, path) { fetch(base + path) }` below a `// --- private ---`
///   line, called only by same-file callers (`getGroupInfo() → request("GET", ` + a path template)).
///   Their same-file consume join already worked; only def-collection was export-gated. Strictly
///   additive (new keyed http consumes appear for these shapes, no existing shape changes), but cached
///   entries from before this marker must not be served as fresh since they lack the new facts.
pub const PARSER_FINGERPRINT: &str = "typescript/swc_core-71.0.5/v4+late-resolve-v1+oazapfts-v1+trpc-v1+router-mounts-v1+wrapper-calls-v1+hono-client-v1+router-mounts-v2+db-table-consume-v1+query-call-sites-v1+store-binding-removed-v1+write-sites-v1+reexport-edges-v1+dynamic-import-edges-v1+nest-global-prefix-v1+jsx-in-js-v1+base-relative-egress-v1+query-drop-v1+controller-prefix-ref-v1+cond-literal-fanout-v1+express-router-vocab-v2+angular-httpclient-v1+str-concat-url-v1+loop-spans-v1+pathname-dispatch-v1+base-carrier-drop-v1+body-shape-v1+axios-defaults-base-v1+oazapfts-removed-v1+express-middleware-v1+db-table-bare-receiver-v1+intra-file-wrapper-v1";

/// POLICY VOCABULARY — array-iteration callback methods whose first function-shaped argument runs once
/// per element (`Array.prototype` iteration methods only; `Map`/`Set`/`for...in` etc. are out of scope).
/// Consumed by [`extract_loop_spans`] to project the callback-argument span as a loop body, alongside
/// real `for`/`while`/`do-while` statement spans, feeding `MethodScan::trigger_in_loop`. Deliberately a
/// plain identifier-property vocabulary (no receiver-type proof, same "syntactic, not type-checked"
/// tradeoff every other adapter in this crate makes) — a same-named method on an unrelated type (a
/// custom `.map()` on a non-array object) is a false positive this vocabulary accepts.
pub const ARRAY_ITERATION_METHODS: &[&str] = &[
    "map",
    "forEach",
    "filter",
    "reduce",
    "reduceRight",
    "flatMap",
    "some",
    "every",
    "find",
    "findIndex",
];
