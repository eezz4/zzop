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
mod call_sites;
#[cfg(test)]
mod call_sites_tests;
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
mod string_literals;
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
// `CONSOLE_WRITE_METHODS` is policy vocabulary and would sit beside `ARRAY_ITERATION_METHODS` below by
// this crate's convention; it lives in its own module instead because that convention lost to the
// 300-line cap on this file. Re-exported here so the crate-level spelling a policy pin cites is the
// same one either way.
pub use call_sites::{extract_call_sites, CONSOLE_WRITE_METHODS};
pub use dead_export_facts::{parse_dead_export_facts, DeadExportFacts};
pub use function_spans::extract_function_spans;
pub use ident_refs::parse_local_identifier_refs;
pub use imports::parse_imports;
pub use loop_spans::extract_loop_spans;
pub(crate) use parse::{line_of, parse_module, parse_with_cm};
pub use parse::{parse_count, parse_ok, reset_parse_count};
pub use project::{build_common_ir, count_loc};
pub use re_exports::{parse_dynamic_imports, parse_re_exports};
pub use sfc_imports::extract_sfc_script_imports;
pub use signature_refs::parse_exported_signature_names;
pub use string_literals::extract_string_literals;
pub use symbols::{parse_symbols, parse_symbols_with_vocab};

/// Cache-bust token for `zzop-cache`: `parser-id/toolchain/last-change-version`.
///
/// **This string is an ID, not a version — it no longer has to be bumped.** `crates/engine/build.rs`
/// hashes this crate's whole dependency closure into the cache key beside it, so a change to any
/// source here invalidates on its own. What is left is the part a person reads in a cache path or a
/// bug report: which frontend parsed the file. Change it when the FRONTEND changes; correctness no
/// longer depends on remembering.
///
/// ⚠ The `swc_core-71.0.5` segment is a HUMAN LABEL, not a pin, and this doc used to claim it "must
/// match this crate's `Cargo.toml` pin exactly". There is no such pin: the manifest declares
/// `swc_core = "71.0.5"`, a CARET range, so `cargo update` can resolve 71.9.x while this label still
/// reads 71.0.5. What actually invalidates on that upgrade is `FP_ENGINE`, which hashes `Cargo.lock`
/// — the resolved version — as part of the suffix on every arm of `cache::parser_fingerprint`.
/// (Contrast `zzop-engine`'s `ignore = "=0.4.27"`, which IS an exact pin and says so with `=`.)
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

use zzop_core::recognizer::{channel, FrameworkRecognizer};

/// Frameworks this parser recognizes — see [`zzop_core::recognizer`] for what a declaration does and
/// does not claim.
///
/// This is the longest list in the workspace, and the reason is worth stating so it is not read as a
/// coverage target: roughly half of these have NO counterpart in another ecosystem (tRPC, Next.js
/// route files, Hono, Nest decorators are TypeScript-shaped). "Recognizer parity with TypeScript" is
/// therefore not a goal for any other parser — the goal is layer-2 population coverage per ecosystem
/// (`parser-expansion.md` §0), and the populations differ.
///
/// Several adapter MODULES are deliberately absent here because they are mechanisms rather than
/// frameworks — `class_shapes`, `wrapper_calls`, `global_prefix` and the `client_base` pair refine or
/// resolve what the framework rows above already found, and declaring them would answer "does zzop
/// know my stack" with our own module names. `pathname_dispatch` used to be listed in that sentence
/// and was moved OUT of it on 2026-08-01: it recognizes framework-less servers on its own evidence and
/// emits its own provides, so calling it a mechanism was simply wrong (see its row below).
pub const FRAMEWORK_RECOGNIZERS: &[FrameworkRecognizer] = &[
    FrameworkRecognizer {
        framework: "express",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    FrameworkRecognizer {
        framework: "nestjs",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    // Nest fills the auth-evidence channel twice over: `controller_decorators`' `@UseGuards` lines and
    // `nest_middleware`'s `forRoutes` patterns both feed the decorator-guard side channel that exempts
    // routes from `mutating-route-no-auth` — guard evidence, not io. Express/hono deliberately do NOT
    // carry this row: their guard words ride INSIDE the mount fragments and surface as `auth-guarded`
    // attributes on their own `io.provides` at compose time, not as a separate side channel.
    FrameworkRecognizer {
        framework: "nestjs",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::AUTH_EVIDENCE],
    },
    FrameworkRecognizer {
        framework: "next.js",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    // Framework-LESS servers that route by comparing `url.pathname` against string literals — raw
    // Cloudflare Workers, Node `http.createServer`, Deno/Bun `serve` (`pathname_dispatch`). The row is
    // spelled after the SHAPE rather than after a package because there is no package to name: the
    // honest claim is "a server that dispatches on `url.pathname` is recognized". Until 2026-08-01 this
    // module was carried as a `NOT_A_FRAMEWORK` exemption reading "route-shape heuristic shared by
    // several framework rows", which was false in both halves — no other row consumes it, and it mints
    // its own `io.provides` from its own per-function evidence gates.
    FrameworkRecognizer {
        framework: "pathname dispatch",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    // Hono fills BOTH sides of the join, and until 2026-08-01 this list said it filled one. The
    // provide side is `router_mounts`' `new Hono()` / `: Hono` receiver vocabulary, whose verb and
    // mount fragments the engine composes into `http` provides (`compose_router_mount_provides`); the
    // consume side is `hono_client`'s typed RPC calls. Worth naming what this was: `emits` exists
    // precisely so a parser cannot look whole while filling half a join, and this was the FIRST wrong
    // answer the field itself produced — the mechanism that catches an under-claiming PARSER does not
    // catch an under-claiming ROW, because nothing binds a row's channel set to the modules behind it.
    FrameworkRecognizer {
        framework: "hono",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    FrameworkRecognizer {
        framework: "hono",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "trpc",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    FrameworkRecognizer {
        framework: "trpc",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "typeorm",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::DB],
    },
    FrameworkRecognizer {
        framework: "prisma client",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::DB],
    },
    FrameworkRecognizer {
        framework: "raw sql",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::DB],
    },
    FrameworkRecognizer {
        framework: "axios",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "fetch",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    // `ky` and `$fetch` were MISSING from this list until 2026-08-01, which made the disclosure
    // understate what this build knows — the opposite drift direction from the one
    // `rule_contracts::recognizer_drift` catches, and invisible to it: that guard binds MODULES to
    // rows, and both of these live inside the already-declared `egress` module. The residual is
    // therefore known and stated rather than guessed at: a module's row set is guarded, the client
    // VOCABULARY inside one is not, so widening `egress/matchers.rs` needs a row added here by hand.
    FrameworkRecognizer {
        framework: "ky",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "$fetch",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    // The rest of that same residual, paid down 2026-08-01: `egress/angular.rs` and
    // `egress/generated_client.rs` are two more client recognizers living inside the declared `egress`
    // module, and neither had a row. `angular` is the dependency-injected `HttpClient` idiom, hard-gated
    // on the file importing `@angular/common/http`; the generated row covers the three openapi codegen
    // families whose call sites carry the URL as a request-descriptor PROPERTY rather than an argument
    // (swagger-typescript-api's `.request({ path, method })`, openapi-typescript-codegen's
    // `__request(OpenAPI, { url, method })`, `@hey-api/openapi-ts`'s `.get({ url })`). Both tag their
    // consumes with their own `IoConsume::client` value (`"angular"`, `"generated"`), which is the same
    // vocabulary a reader of this list is asking about.
    FrameworkRecognizer {
        framework: "angular",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "openapi generated client",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
];
