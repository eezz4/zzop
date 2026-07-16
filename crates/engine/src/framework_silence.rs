//! Coverage self-report: five lexical, extractor-independent tripwires that flag when a tree LOOKS like
//! it carries a framework surface zzop cannot see, so cross-layer joins would otherwise go silently dark
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
//!
//! All five are per-tree self-report `warnings: Vec<String>` strings (not `Finding`s — no rule id, no
//! catalog sync needed); over-disclosure is safe, silence is fatal (the coverage-disclosure decision doc's
//! governing principle) — each function is additive and may fire independently of the others.
//!
//! Module layout — one file per tripwire (S1/S2/S3/S4/S5), `MIN_PROVIDES_FLOOR` defined once in
//! `controller_silence` (S1) and shared by the other four:
//! - [`controller_silence`](self) — S1 + `MIN_PROVIDES_FLOOR`.
//! - [`server_framework_import`](self) — S2 + [`provide_blind_sources`], the run-wide severity-gate helper
//!   `cross-layer/unprovided-mutation-call` also reuses.
//! - [`committed_spec_io_silence`](self) — S3 + `IO_NEAR_ZERO_FLOOR`.
//! - [`client_library_import`](self) — S4.
//! - [`builtin_fetch`](self) — S5 + `FETCH_CALL_SITES_MIN`.

mod builtin_fetch;
mod client_library_import;
mod committed_spec_io_silence;
mod controller_silence;
mod server_framework_import;
#[cfg(test)]
mod tests;

pub use builtin_fetch::builtin_fetch_lexical_warning;
pub use client_library_import::client_library_import_warning;
pub use committed_spec_io_silence::committed_spec_io_silence_warning;
pub(crate) use committed_spec_io_silence::IO_NEAR_ZERO_FLOOR;
pub use controller_silence::controller_silence_warning;
pub(crate) use controller_silence::MIN_PROVIDES_FLOOR;
pub use server_framework_import::{provide_blind_sources, server_framework_import_warning};
