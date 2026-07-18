//! Coverage self-report: lexical, extractor-independent tripwires that flag when a tree LOOKS like it
//! carries a framework surface zzop cannot see, so cross-layer joins would otherwise go silently dark
//! with NO honesty channel firing at all (the gap dogfood round 9 found: a whole vue<->express pair went
//! ~totally blind and nothing in `warnings` said so).
//!
//! - S1 [`controller_silence_warning`]: DECORATOR-style controller idioms (Nest-, `@n8n/decorators`-, and
//!   Spring-style — the shapes `zzop_parser_typescript::adapters::controller_decorators` currently
//!   teaches) matched lexically, gated on NEAR-zero (not exact-zero) extracted `http` provides. Round 14's
//!   Angular-FE x Spring-BE pair lost 17/19 routes to a parser limit but still had 2 lexically-extracted
//!   provides tree-wide, which silenced an exact-zero gate; S1 now shares S2's `MIN_PROVIDES_FLOOR`
//!   near-zero floor rather than gating on exactly zero.
//! - S2 [`server_framework_import_warning`]: a server-framework PACKAGE IMPORT (express, koa, fastify,
//!   ...) present while extracted `http` provides stay near-zero. Closes the METHOD-CALL registration
//!   idiom S1's decorator regex structurally cannot see — round 9's be-express tree registered routes as
//!   `router.get('/x', ...)`, never a decorator, and still had 1 extracted provide, which would have
//!   short-circuited an exact-zero gate like S1's.
//! - S3 [`committed_spec_io_silence_warning`]: a committed OpenAPI/Swagger spec sits in the tree while
//!   this tree's io stays near-zero in BOTH directions (provides AND keyed consumes). Round 9's fe-vue
//!   tree talked to its backend through a client generated FROM `src/services/openapi.yml`, so the
//!   consume extractor (which reads call-site literals, not generated SDK internals) saw nothing.
//! - S4 [`client_library_import_warning`]: an http-CLIENT PACKAGE IMPORT (axios, `@angular/common/http`,
//!   ...) present while extracted `http` consumes stay near-zero — the consume-side dual of S2, closing
//!   the gap that round 14's Angular-FE tree exposed: ~15 real `HttpClient` call sites, 0 extracted
//!   consumes, and no consume-side honesty channel at all until now.
//! - S5 [`builtin_fetch_lexical_warning`]: a lexical census of builtin `fetch(` call tokens over the
//!   tree's js/ts sources, gated on near-zero KEYED `http` consumes. Closes the gap S4's own doc names:
//!   builtin `fetch` is a global, not a module specifier, so a hand-rolled wrapper over `fetch` has no
//!   import for S4 to anchor on — a live tree extracted 1 of ~10 fetch-style consumes with NO warning.
//! - S6 [`orm_schema_silence_warning`]: an ORM-schema package/import (TypeORM, Sequelize, Drizzle, JPA,
//!   SQLAlchemy, GORM) present while zero `db-table` io facts (provides + consumes) were extracted
//!   tree-wide — EXACT zero, not near-zero, matching the observed gap verbatim: a live NestJS repo full of
//!   TypeORM `@Entity` decorators produced zero db-table facts and no warning at all. Deliberately
//!   excludes Prisma (this engine's native db-table path already covers it) — see that module's doc.
//! - S7 [`fetch_wrapper_call_site_warning`]: the wrapper-indirection dual of S5 — a lexical two-pass
//!   census that finds a hand-rolled fetch-wrapper module (exports `get`/`post`/`put`/`del`-shaped
//!   bindings over one internal `fetch(` call) and counts cross-file call sites of those exports from
//!   OTHER files that import it, gated on the same near-zero KEYED `http` consumes floor S5 uses. Closes
//!   the gap S5's own tree-wide token count structurally cannot: a tree that funnels 20+ real call sites
//!   through one wrapper still shows only ONE literal `fetch(` token tree-wide (inside the wrapper
//!   itself), which can sit below S5's own `FETCH_CALL_SITES_MIN` floor even though the real call-site
//!   surface is large (blind-field test R10's fe-svelte: `src/lib/api.js`, 20+ callers under
//!   `src/routes/**`).
//!
//! All seven are per-tree self-report `warnings: Vec<String>` strings (not `Finding`s — no rule id, no
//! catalog sync needed); over-disclosure is safe, silence is fatal (the coverage-disclosure decision doc's
//! governing principle) — each function is additive and may fire independently of the others.
//!
//! S5 and S7 are additionally PER-APP: in a monorepo tree (one `package.json` per app) the census gates
//! per app-root, so a healthy sibling app's keyed consumes no longer MASK a dark app — the fired warning
//! NAMES the below-floor app. Both also apply an internal-intent filter (`egress_intent`) so an app that
//! only ever calls ABSOLUTE external services (a CDN, a third-party API — nothing internal to join) stays
//! silent. `builtin_fetch_census`/`fetch_wrapper_census` may each emit MULTIPLE per-app entries plus an
//! optional tree-wide FALLBACK (an internal-egress mass that split across packages below every per-app
//! floor but whose aggregate still clears the call-site floor).
//!
//! Module layout — one file per tripwire (S1/S2/S3/S4/S5/S6/S7), `MIN_PROVIDES_FLOOR` defined once in
//! `controller_silence` (S1) and shared by S2/S3/S4/S5/S7 (S6 uses its own exact-zero gate, no floor):
//! - [`controller_silence`](self) — S1 + `MIN_PROVIDES_FLOOR`.
//! - [`server_framework_import`](self) — S2 + [`provide_blind_sources`], the run-wide severity-gate helper
//!   `cross-layer/unprovided-mutation-call` also reuses.
//! - [`committed_spec_io_silence`](self) — S3 + `IO_NEAR_ZERO_FLOOR`.
//! - [`client_library_import`](self) — S4.
//! - [`builtin_fetch`](self) — S5 + `FETCH_CALL_SITES_MIN` (also reused by S7) + `builtin_fetch_census`.
//! - [`orm_schema_silence`](self) — S6.
//! - [`fetch_wrapper`](self) — S7 + `fetch_wrapper_census`.
//! - [`egress_intent`](self) — internal-intent classifier shared by the S5/S7 censuses.
//! - [`app_buckets`](self) — per-app-root bucketing (`app_roots`/`nearest_app_root`/`keyed_http_by_root`).

mod app_buckets;
mod builtin_fetch;
mod client_library_import;
mod committed_spec_io_silence;
mod controller_silence;
mod egress_intent;
mod fetch_wrapper;
mod orm_schema_silence;
mod server_framework_import;
#[cfg(test)]
mod tests;

pub(crate) use app_buckets::{app_roots, keyed_http_by_root};
// `builtin_fetch_lexical_warning`/`fetch_wrapper_call_site_warning` are NOT re-exported here: their
// only in-crate consumer is now the census (which calls them on its single-package path via the local
// module), and `framework_silence`'s own unit tests reach them via `super::builtin_fetch::` /
// `super::fetch_wrapper::` directly. A crate-level re-export would be dead (`mod framework_silence` is
// private, so there is no external API surface to keep them on).
pub use builtin_fetch::builtin_fetch_census;
pub use client_library_import::client_library_import_warning;
pub use committed_spec_io_silence::committed_spec_io_silence_warning;
pub(crate) use committed_spec_io_silence::IO_NEAR_ZERO_FLOOR;
pub use controller_silence::controller_silence_warning;
pub(crate) use controller_silence::MIN_PROVIDES_FLOOR;
pub use fetch_wrapper::fetch_wrapper_census;
pub use orm_schema_silence::orm_schema_silence_warning;
pub use server_framework_import::{provide_blind_sources, server_framework_import_warning};
