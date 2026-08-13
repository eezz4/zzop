//! `zzop-facade` — the ANALYSIS-MEANING CONTRACT LAYER: the engine's pure JSON boundary (`analyze` /
//! `analyzeTrees` / `analyzeEnvelope`) AND the read-only meaning lookups that sit on top of engine and
//! rule data — `queryIo`'s verdict vocabulary, rule-pack validation, `explain`, version reporting. All
//! of it napi-free (plain `&str -> Result<String, String>` / `-> String`) so it compiles and has
//! a normal `#[test]` surface under the workspace's default `gnu` toolchain with no feature flags at
//! all. The name suggests thinness, but this is NOT the thin layer — the thin things are the two
//! products in `packages/`. Defaults live in the HOST (`zzop-config`'s mapper), not here —
//! with exactly one deliberate exception: the envelope bundled-pack seed (`envelope.rs`), because the
//! envelope path is the one entry point no host config front-end covers. Corollary: `zzop-config` must
//! never depend on this crate's request types (that edge would be a cycle) — if typed request sharing
//! is ever wanted, the structs move DOWN (core or a small wire crate), never config -> facade.
//!
//! The crate's sole direct consumer is `zzop-summary`, which the two Node-free bins (`zzop` from
//! `packages/cli-bin`, `zzop-mcp` from `packages/mcp`) call in turn — no napi, no Node process, and
//! neither product depends on this crate directly (the entry points they use verbatim are re-exported
//! from `zzop_summary`). (This crate was split off as its own `rlib`-only crate because a
//! since-removed napi addon crate's `cdylib` half failed to link under the local `gnu` toolchain once
//! its `#[napi]` surface was compiled in, which would have poisoned any rlib-only dependent. The addon
//! is gone, but keeping the facade a standalone napi-free `rlib` still gives every consumer a normal
//! `#[test]` surface under the default toolchain with no feature flags.)
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
//! - `explain` — `zzop explain <rule-id>`'s read-only lookup over the DSL rule data compiled into the
//!   binary plus the live native-analysis registry. Same KIND of work as `query`'s verdict vocabulary
//!   and `rule_pack`'s validation — a pure read whose answer is a MEANING — which is why it lives here
//!   rather than three levels up (2026-07-26 `crates/host` teardown).
//! - `version` — `version()` and `version_string()`, the single owner of the reported version.

mod analyze;
mod config;
mod envelope;
mod explain;
mod output;
mod query;
mod query_coverage;
mod query_file;
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
#[cfg(test)]
mod test_region_promise_tests;

pub use analyze::{analyze_json, analyze_trees_json};
pub use envelope::{analyze_envelope_json, validate_envelope_only_json};
pub use explain::{explain, explain_with_config, native_analysis_ids};
pub use query::query_io_json;
pub use query_coverage::query_coverage_json;
pub use query_file::{query_file_json, FILE_VERDICTS};
pub use request::{
    AnalyzeRequest, AnalyzeTreesRequest, CommitSubjectPatternRequest, CommitTypePatternRequest,
    EnvelopeAnalyzeRequest, GitOptionsRequest, MountEntryRequest, PacksDir,
};
pub use rule_pack::validate_rule_pack_json;
pub use version::{version, version_string};
/// The DSL shared test-paths fragment, compiled — re-exported for zzop-summary, whose layering
/// (no shipped dependency below this crate) is deliberate, and whose first-screen ordering of
/// test-path findings (2026-08-09 U78 ruling) must read the SAME pattern the packs expand as
/// their test-path exclusions. One string, one owner, two consumers that cannot disagree.
pub use zzop_core::dsl::test_path_re;
/// The dep graph's membership rule, in one sentence with ONE owner (`zzop_core::ir`). Re-exported for
/// the same layering reason as the disclosure views below: `zzop-summary` publishes dep-graph-derived
/// numbers (`fanIn`/`fanOut`/`degree` on the cosmograph points table) and must be able to disclose what
/// graph they describe without skipping a layer to reach `zzop-core`, which is deliberately not a
/// shipped dependency of that crate. Re-exported, never restated — a second copy of this sentence is a
/// second thing that can drift.
pub use zzop_core::DEP_GRAPH_RESOLVED_ONLY;
/// The silent-failure-class registry's two DERIVED views — its full text as a contract document, and
/// its per-status tallies. Re-exported rather than wrapped, and re-exported HERE rather than reached
/// for directly, because this is the analysis-MEANING layer (the same reason `explain` and `version`
/// live here): `zzop-summary` shapes the reply and serves the contract table, and it must not skip a
/// layer to `zzop-engine` to do either — the facade/summary split is what `docs/contracts/
/// surface-parity.json` and its metatests rest on. Both views read one `const` registry
/// (`zzop_engine::BLINDNESS_REGISTRY`), which is what lets the reply carry counts while the text ships
/// once: a second owner of either would be a tally that can drift from the prose it summarizes.
pub use zzop_engine::{disclosure_contract_text, disclosure_counts};

/// Re-exported for the same reason as the two above: a surface that wants to SAY how many health
/// scores there are must read the registry, not retype the number. It was retyped, and v0.30.0's
/// removal of `typeSafety`/`lod` left `zzop graph --domain risk` printing "the 17 structural health
/// scores" over a table of 15 — with a test pinning the literal, so the guard protected the lie.
/// `crates/summary` has no `zzop-metrics` dependency of its own and does not need one for this.
pub use zzop_metrics::SCORE_MEANINGS;
