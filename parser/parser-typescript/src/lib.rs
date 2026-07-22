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
mod asset_refs;
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
mod sfc_imports;
mod symbol_shapes;
mod symbols;
#[cfg(test)]
mod symbols_tests;
#[cfg(test)]
mod test_util;

pub use adapters::class_shapes::extract_class_shape_fragments;
pub use adapters::client_base::{extract_client_base_prefix_marker, CLIENT_BASE_PREFIX_KIND};
pub use adapters::client_base_generated::extract_generated_client_base_prefix_marker;
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
pub use adapters::entity_decorators::extract_entity_db_table_provides;
pub use adapters::global_prefix::{extract_global_prefix_marker, NEST_GLOBAL_PREFIX_KIND};
pub use adapters::hono_client::extract_hono_client_consumes;
pub use adapters::nest_middleware::{extract_nest_forroutes_guarded, ForRoutesPattern};
pub use adapters::next_pages_api::{scan_pages_api_handler, PagesApiHandlerScan};
pub use adapters::pathname_dispatch::extract_pathname_dispatch_provides;
pub use adapters::router_mounts::extract_router_mount_fragments;
pub use adapters::trpc_consume::extract_trpc_consumes;
pub use adapters::trpc_router::extract_procedure_router_fragments;
pub use adapters::typeorm_repository::extract_typeorm_repository_consumes;
pub use adapters::wrapper_calls::extract_wrapper_fragments;
pub use lang::calls::parse_calls;
pub use lang::resolve::{
    build_dep, build_dep_with_workspace, resolve_file, resolve_file_with_workspace, try_ext,
    TsconfigPaths, WorkspacePkg, RESOLVE_EXTS,
};
pub use lang::write_site::{
    write_sites_for_symbol, DEFAULT_ORM_RECEIVER_PATTERN, DEFAULT_WRITE_METHODS,
};

pub use asset_refs::parse_asset_refs;
pub use ident_refs::parse_local_identifier_refs;
pub use imports::parse_imports;
pub use loop_spans::extract_loop_spans;
pub use parse::parse_ok;
pub(crate) use parse::{line_of, parse_module, parse_with_cm};
pub use project::{build_common_ir, count_loc};
pub use re_exports::{parse_dynamic_imports, parse_re_exports};
pub use sfc_imports::extract_sfc_script_imports;
pub use symbols::parse_symbols;

/// Cache-bust token for `zzop-cache`: `parser-id/pinned-toolchain/last-change-version`. The
/// `swc_core-71.0.5` segment must match this crate's `Cargo.toml` pin exactly (an swc upgrade changes
/// extraction → must restamp). The trailing `CARGO_PKG_VERSION` is restamped whenever this crate's
/// projected IR shape changes; an unchanged release keeps the old value so warm TS caches survive the
/// upgrade (2026-07-22 version reform — the "what changed" narrative lives in git, not this string).
pub const PARSER_FINGERPRINT: &str = "typescript/swc_core-71.0.5/0.21.0";

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
