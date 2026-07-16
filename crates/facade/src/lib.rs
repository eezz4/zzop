//! `zzop-facade` — the engine's pure JSON facade: the actual `analyze` / `analyzeTrees` / `version`
//! logic, kept napi-free (plain `&str -> Result<String, String>` / `-> String`) so it compiles and has
//! a normal `#[test]` surface under the workspace's default `gnu` toolchain with no feature flags at
//! all. Defaults live in the HOSTS (JS wrapper `withDefaults`, `zzop-config`'s mapper), not here —
//! with exactly one deliberate exception: the envelope bundled-pack seed (`envelope.rs`), because the
//! envelope path is the one entry point no host config front-end covers. Corollary: `zzop-config` must
//! never depend on this crate's request types (that edge would be a cycle) — if typed request sharing
//! is ever wanted, the structs move DOWN (core or a small wire crate), never config -> facade.
//!
//! Two consumers share this crate:
//! - `zzop-napi` re-exports every function from here and wraps each one with a thin `#[napi]` shim
//!   under its default-off `addon` feature (`packages/native/src/addon.rs`) — the Node addon build.
//! - `zzop-mcp`, a Node-free binary, calls these functions directly — no napi, no Node process.
//!
//! It lives in its own `rlib`-only crate, separate from `zzop-napi`, because cargo builds a
//! dependency's `cdylib` target even on an `rlib` dependency edge: `zzop-napi`'s `cdylib` half (the
//! Node addon artifact) fails to link under the local `gnu` toolchain with "export ordinal too large"
//! once its `#[napi]` surface is compiled in, and that failure would poison any crate that merely
//! depended on `zzop-napi` for its plain-Rust logic — even one, like `zzop-mcp`, that never touches
//! napi at all. Splitting the napi-free logic into a separate `rlib` crate sidesteps the cdylib link
//! step entirely for every consumer except the Node addon build itself.
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
//!   check — the shared core behind `validate_rule_pack` and `zzop pack validate`).
//! - `version` — the `version()` entry point.

mod analyze;
mod config;
mod envelope;
mod output;
mod query;
mod request;
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
