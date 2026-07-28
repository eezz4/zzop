//! zzop-summary — the shared summary/shaping crate behind both zzop host products (`packages/cli-bin`'s
//! `zzop` CLI and `packages/mcp`'s `zzop-mcp` server). Architectural rule this crate exists to enforce:
//! hosts are thin protocol facades — they extract arguments from whatever wire format they speak (MCP
//! `tools/call` JSON, CLI argv, ...) and call the functions here; ALL summary/shaping/filter/
//! warning-merge logic lives in this crate so it cannot drift per-host. This is a direct response to a
//! recurring defect class: AI-agent-driven development batches kept reintroducing surface-drift bugs (a
//! cap forgotten in one host's copy of the shaping logic, a warning merged in one host and dropped in
//! another) because the shaping logic used to live inside the host package itself. A host that
//! reimplements any of this instead of calling it is exactly the drift this split exists to close.
//!
//! Both products depend on THIS crate and nothing below it: the `zzop-facade` entry points a host still
//! needs verbatim (`explain`, `version`, the two offline validators) are re-exported at the bottom of
//! this file rather than wrapped, so "a host needs only `zzop-summary`" stays true without a
//! pass-through layer that could drift.
//!
//! Module map:
//! - `args`   — argument extraction from an MCP `tools/call` `arguments` object
//!   (`required_string`/`optional_string`/`optional_string_array`); every declared-type violation is a
//!   named error, never a silent fallback. This is the ONE module here that is wire parsing and nothing
//!   else, and it exists for the MCP host alone — the CLI reaches the same handlers through wire-neutral
//!   constructors (`output::FindingFilters::new`) instead of assembling JSON it does not speak. (One
//!   shaping module also touches the wire, through these helpers and only these:
//!   `output::FindingFilters::from_args`, the JSON front door onto the wire-neutral `new`.)
//! - `output` — tool-output shaping: `FindingFilters`, capped lists, explicit truncation disclosure,
//!   cross-layer bucket-key shaping (the token-bomb guard behind every reply).
//! - `contracts` — the compile-time embedded authoring-contract documents (`zzop://contract/<name>` over
//!   MCP, `zzop contract [<name>]` from a terminal): the ONE table both surfaces resolve names through.
//! - `siblings` — sibling-directory scope disclosure for `cross_summary`.
//! - `suggest`  — deterministic nearest-key fallback for `endpoint_summary`'s `not-found` suggestions.
//! - `config_warnings` — facade-level `configWarnings` merge helper shared by `analyze_summary`/
//!   `cross_summary`.
//! - `analyze`  — `analyze_summary`: one-tree analysis (config auto-discovery + facade call + summary
//!   assembly); `analyze_envelope_summary`: Mode A full-envelope analysis (no filesystem root — a
//!   minimal `"{}"` config drives the same facade call), sharing the tree-mode path's post-facade
//!   shaper.
//! - `cross`    — `cross_summary`: multi-tree cross-layer join summary.
//! - `facts`    — `facts_json`: the post-assembly FACT DUMP (`zzop facts`) — per-tree `CommonIr` plus
//!   the whole cross-layer join, uncapped, for a user's own rule program to read. The emit half of the
//!   custom-rule extension point; zzop executes nothing and ingests nothing (see its module doc).
//! - `graph`    — `graph_mermaid`: the cross-layer JOIN serialized as a mermaid flowchart (`zzop
//!   graph`) for an EXTERNAL renderer to draw. The one rendering-adjacent surface here, and
//!   deliberately format-serialization only: scoped (`--scope`/`--top`), with every cap and filter
//!   disclosed in the document itself.
//! - `manifest` — `manifest_json`/`diff_manifests_json`: the structural CONTRACT MANIFEST of a
//!   cross-layer run and the delta between two of them (`zzop manifest` / `zzop diff`). Identity
//!   only, uncapped, sorted — the surface that stays readable ABOVE this module's caps, where two
//!   capped summaries agree on the counts and cannot say which route left the join.
//! - `endpoint` — `endpoint_summary`: the `check_endpoint` query core (tree resolution + facade query +
//!   suggestion fallback).
//!
//! Request ASSEMBLY is not a module here — which trees a call is about (`path` XOR `paths` XOR
//! `configPath`, the single-tree-vs-join judgment, zero-config paths mode) lives in `zzop_config::trees`,
//! next to the config vocabulary that decides what a valid tree declaration is; host-boundary path
//! absolutization lives beside it in `zzop_config::paths`.

mod analyze;
pub mod args;
mod config_warnings;
pub mod contracts;
mod cross;
#[cfg(test)]
mod cross_test;
mod endpoint;
mod facts;
mod file;
mod graph;
mod manifest;
pub mod output;
mod siblings;
mod suggest;

pub use analyze::{analyze_envelope_summary, analyze_summary};
pub use cross::cross_summary;
pub use endpoint::endpoint_summary;
pub use facts::facts_json;
pub use file::file_summary;
pub use graph::{
    graph_cosmograph, graph_mermaid, CosmographOutput, GraphDomain, GraphFormat, DEFAULT_GRAPH_TOP,
};
pub use manifest::{diff_manifests_json, manifest_json};
pub use output::FindingFilters;
// Verbatim `zzop-facade` entry points, re-exported (never wrapped) so a host product needs only this
// crate: `explain`, `version` and `version_string` are pure reads over engine/rule data the facade owns,
// and the two validators are structure-only checks with no shaping of this crate's own. Only what a host
// actually calls is re-exported; `version_string` (the diagnostic form: version + every parser
// fingerprint) earned its place back on 2026-07-27, when `zzop version --verbose` and
// `zzop-mcp version --verbose` became its first host callers — this crate had been reaching it directly
// for the `tool` field of `manifest`/`facts`/`graph` while both binaries could only report the bare form.
pub use zzop_facade::{
    explain, validate_envelope_only_json, validate_rule_pack_json, version, version_string,
};
