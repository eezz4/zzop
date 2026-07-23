//! zzop-summary — the shared summary/shaping crate behind every zzop host (`zzop-mcp` today, a future
//! full-CLI binary tomorrow). Architectural rule this crate exists to enforce: hosts are thin protocol
//! facades — they extract arguments from whatever wire format they speak (MCP `tools/call` JSON, CLI
//! argv, ...) and call the functions here; ALL summary/shaping/filter/warning-merge logic lives in this
//! crate so it cannot drift per-host. This is a direct response to a recurring defect class:
//! AI-agent-driven development batches kept reintroducing surface-drift bugs (a cap forgotten in one
//! host's copy of the shaping logic, a warning merged in one host and dropped in another) because the
//! shaping logic used to live inside the host package itself. A host that reimplements any of this
//! instead of calling it is exactly the drift this split exists to close.
//!
//! Module map:
//! - `args`   — shared, MCP-protocol-agnostic `tools/call`-shaped argument extraction (`required_string`/
//!   `optional_string`/`optional_string_array`); every declared-type violation is a named error, never a
//!   silent fallback.
//! - `output` — tool-output shaping: `FindingFilters`, capped lists, explicit truncation disclosure,
//!   cross-layer bucket-key shaping (the token-bomb guard behind every reply).
//! - `paths`  — host-boundary path absolutization (the `zzop-config` mapper's absolute-root contract).
//! - `trees`  — zero-config "paths mode" tree building, shared by `cross_summary`/`endpoint_summary`.
//! - `siblings` — sibling-directory scope disclosure for `cross_summary`.
//! - `suggest`  — deterministic nearest-key fallback for `endpoint_summary`'s `not-found` suggestions.
//! - `config_warnings` — facade-level `configWarnings` merge helper shared by `analyze_summary`/
//!   `cross_summary`.
//! - `analyze`  — `analyze_summary`: one-tree analysis (config auto-discovery + facade call + summary
//!   assembly); `analyze_envelope_summary`: Mode A full-envelope analysis (no filesystem root — a
//!   minimal `"{}"` config drives the same facade call), sharing the tree-mode path's post-facade
//!   shaper.
//! - `cross`    — `cross_summary`: multi-tree cross-layer join summary.
//! - `endpoint` — `endpoint_summary`: the `check_endpoint` query core (tree resolution + facade query +
//!   suggestion fallback).
//!
//! `validate_envelope_only_json`/`validate_rule_pack_json` are thin re-exports of `zzop-facade`'s own
//! structure-only validators — pure pass-through, no shaping logic of this crate's own, re-exported so a
//! host needs only this crate (not `zzop-facade` directly) to dispatch its full tool surface.

mod analyze;
pub mod args;
mod config_warnings;
mod cross;
#[cfg(test)]
mod cross_test;
mod endpoint;
pub mod output;
mod paths;
mod siblings;
mod suggest;
mod trees;

pub use analyze::{analyze_envelope_summary, analyze_summary};
pub use cross::cross_summary;
pub use endpoint::endpoint_summary;
pub use output::FindingFilters;
pub use zzop_facade::{validate_envelope_only_json, validate_rule_pack_json};
