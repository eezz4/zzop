//! `zzop-facade` — the engine's pure JSON facade: the actual `analyze` / `analyzeTrees` / `version`
//! logic, kept napi-free (plain `&str -> Result<String, String>` / `-> String`) so it compiles and has
//! a normal `#[test]` surface under the workspace's default `gnu` toolchain with no feature flags at
//! all. Defaults live in the HOST (`zzop-config`'s mapper), not here —
//! with exactly one deliberate exception: the envelope bundled-pack seed (`envelope.rs`), because the
//! envelope path is the one entry point no host config front-end covers. Corollary: `zzop-config` must
//! never depend on this crate's request types (that edge would be a cycle) — if typed request sharing
//! is ever wanted, the structs move DOWN (core or a small wire crate), never config -> facade.
//!
//! The consumer today is `zzop-mcp`, a Node-free binary that calls these functions directly — no napi,
//! no Node process. (Until 2026-07-20 a second consumer, the removed `@zzop/native` napi addon
//! `zzop-napi`, re-exported every function behind a thin `#[napi]` shim. This crate was split off as
//! its own `rlib`-only crate because cargo builds a dependency's `cdylib` target even on an `rlib`
//! edge: the addon's `cdylib` half failed to link under the local `gnu` toolchain once its `#[napi]`
//! surface was compiled in, which would have poisoned any crate — like `zzop-mcp` — that depended on
//! it only for the plain-Rust logic. The addon is gone, but keeping the facade a standalone napi-free
//! `rlib` still gives every consumer a normal `#[test]` surface under the default toolchain with no
//! feature flags.)
//!
//! Module layout (every public item is re-exported here, so consumers only ever see `zzop_facade::X`):
//! - `request` — wire-contract request types (`AnalyzeRequest` and friends) + serde defaults.
//! - `config` — request -> `EngineConfig` assembly (pack loading/merging, tree-rooted knobs).
//! - `output` — JSON-serializable views over engine outputs (single-tree, multi-tree, disclosure).
//! - `analyze` — the `analyze`/`analyzeTrees` entry points.
//! - `envelope` — the `analyzeEnvelope`/`validateEnvelopeOnly` entry points.
//! - `query` — the `queryIo` entry point (definitive endpoint/io-key queries over an
//!   already-produced analysis output — the shared core behind `zzop endpoint` and `check_endpoint`).
//! - `rule_pack` — the `validateRulePackOnly` entry point (pre-load, structure-only DSL rule-pack
//!   check — the shared core behind `validate_rule_pack` and `zzop validate-rule-pack`).
//! - `version` — the `version()` entry point.

mod analyze;
mod config;
mod envelope;
mod output;
mod query;
mod request;
mod route_injection;
mod rule_pack;
mod version;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod analyze_tests;
#[cfg(test)]
mod config_tests;
#[cfg(test)]
mod envelope_tests;
#[cfg(test)]
mod packs_tests;
#[cfg(test)]
mod query_e2e_tests;
#[cfg(test)]
mod query_tests;
#[cfg(test)]
mod rule_pack_tests;

pub use analyze::{analyze_json, analyze_trees_json};
pub use envelope::{analyze_envelope_json, validate_envelope_only_json};
pub use query::query_io_json;
pub use request::{
    AnalyzeRequest, AnalyzeTreesRequest, CommitTypePatternRequest, EnvelopeAnalyzeRequest,
    GitOptionsRequest, MountEntryRequest, PacksDir,
};
pub use rule_pack::validate_rule_pack_json;
pub use version::version_string;
