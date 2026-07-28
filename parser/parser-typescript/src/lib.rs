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
mod dead_export_facts;
mod export_aliases;
mod factory;
mod function_spans;
mod ident_refs;
mod imports;
pub mod lang;
mod loop_spans;
mod parse;
mod project;
mod re_exports;
mod sfc_imports;
mod signature_refs;
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
    extract_db_table_consumes, extract_db_table_consumes_with_vocab, extract_query_call_sites,
    extract_query_call_sites_with_vocab, PRISMA_CLIENT_GETTER,
};
pub use adapters::egress::collector::extract_http_egress_with_vocab;
pub use adapters::egress::retry::{RETRY_WRAPPERS, RETRY_WRITE_VERBS};
pub use adapters::egress::{
    base_relative_path, const_map_fragment, extract_http_egress, is_external_url, resolve_raw_path,
};
pub use adapters::entity_decorators::extract_entity_db_table_provides;
pub use adapters::global_prefix::{extract_global_prefix_marker, NEST_GLOBAL_PREFIX_KIND};
pub use adapters::hono_client::extract_hono_client_consumes;
pub use adapters::nest_middleware::{extract_nest_forroutes_guarded, ForRoutesPattern};
pub use adapters::next_pages_api::{scan_pages_api_handler, PagesApiHandlerScan};
pub use adapters::pathname_dispatch::extract_pathname_dispatch_provides;
pub use adapters::raw_sql::extract_raw_sql_db_table_consumes;
pub use adapters::router_mounts::{
    extract_router_mount_fragments, extract_router_mount_fragments_with_vocab, RouterMountVocab,
};
pub use adapters::trpc_consume::extract_trpc_consumes;
pub use adapters::trpc_router::extract_procedure_router_fragments;
pub use adapters::typeorm_repository::extract_typeorm_repository_consumes;
pub use adapters::wrapper_calls::extract_wrapper_fragments;
pub use lang::calls::parse_calls;
pub use lang::resolve::{
    build_dep, build_dep_with_workspace, resolve_file, resolve_file_with_workspace, try_ext,
    TsconfigPaths, WorkspacePkg, RESOLVE_EXTS,
};
pub use lang::write_site::write_sites_for_symbol_with_vocab;
pub use lang::write_site::{
    write_sites_for_symbol, CompiledWriteSiteVocab, WriteSiteVocab, DEFAULT_ORM_RECEIVER_PATTERN,
    DEFAULT_WRITE_METHODS,
};

pub use asset_refs::parse_asset_refs;
pub use dead_export_facts::{parse_dead_export_facts, DeadExportFacts};
pub use function_spans::extract_function_spans;
pub use ident_refs::parse_local_identifier_refs;
pub use imports::parse_imports;
pub use loop_spans::extract_loop_spans;
pub use parse::parse_ok;
pub(crate) use parse::{line_of, parse_module, parse_with_cm};
pub use project::{build_common_ir, count_loc};
pub use re_exports::{parse_dynamic_imports, parse_re_exports};
pub use sfc_imports::extract_sfc_script_imports;
pub use signature_refs::parse_exported_signature_names;
pub use symbols::{parse_symbols, parse_symbols_with_vocab};

/// Cache-bust token for `zzop-cache`: `parser-id/pinned-toolchain/last-change-version`. The
/// `swc_core-71.0.5` segment must match this crate's `Cargo.toml` pin exactly (an swc upgrade changes
/// extraction → must restamp). The trailing `CARGO_PKG_VERSION` is restamped whenever this crate's
/// projected IR shape changes; an unchanged release keeps the old value so warm TS caches survive the
/// upgrade (2026-07-22 version reform — the "what changed" narrative lives in git, not this string).
pub const PARSER_FINGERPRINT: &str =
    "typescript/swc_core-71.0.5/0.22.0+resource-query-v1+trpc-leaf-procedure-v1+dispatch-branch-symbol-v1+exported-signature-names-v1+function-spans-v1+same-file-const-prepend-v1+raw-sql-db-table-v1+same-file-url-binding-v1+same-file-fn-url-v1+retry-wrapper-binding-v1+generated-verb-member-v1+dispatch-verb-order-v1";

/// POLICY VOCABULARY — `Promise.prototype` continuation methods whose function-shaped arguments run on
/// the RESUMED continuation of an async boundary, not inline at the call. Consumed by
/// [`extract_function_spans`] to merge such a callback's span into its call site's line, so a matcher
/// scoping on "nearest function" still sees the boundary token that schedules the callback. Deliberately
/// a plain identifier-property vocabulary (no receiver-type proof, no alias tracking — see
/// [`extract_function_spans`]'s doc for the full narrowness contract).
///
/// **Do not edit this list alone.** `rules/dsl/react/react.json`'s `setstate-after-async-unguarded` spells
/// the same three methods again as the `.(?:then|catch|finally)(` arm of its `async-boundary` pattern — one
/// policy, two spellings, because a JSON pack cannot reference a Rust constant. Narrowing this list while
/// the rule keeps the token silently DELETES findings (the callback is no longer merged into the
/// scheduling call's line, so `after_in_same_function` rejects the pairing) with nothing turning red. The
/// pin that makes that fail loudly is
/// `the_promise_continuation_vocabulary_is_identical_in_the_parser_and_the_react_pack`
/// (`crates/engine/tests/rule_contracts/policy_pins.rs`), which reads the rule's arm out of the shipped
/// pack rather than restating it.
pub const PROMISE_CONTINUATION_METHODS: &[&str] = &["then", "catch", "finally"];

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
