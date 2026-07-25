//! zzop-host — the shared library behind zzop's two Node-free host products: `zzop` (package
//! `zzop-cli-bin`, the CLI — analyze/cross/endpoint/…) and `zzop-mcp` (package `zzop-mcp`, the MCP
//! server over stdio). Both build a thin binary of their own over this lib and dispatch to the same
//! `zzop_host::tools` handlers, so a CLI query and an MCP tool call give the identical answer. Path B
//! of the mcp-distribution decision: the analysis engine, the bundled DSL packs (via the shared
//! `zzop-config` crate), and the authoring contracts all travel inside self-contained executables with
//! no Node runtime.
//!
//! This crate is a THIN PROTOCOL FACADE: `tools` extracts arguments from CLI argv and calls the shared
//! `zzop-summary` crate, which owns EVERY bit of summary/shaping/filter/warning-merge logic. Nothing in
//! this crate re-derives or forks that logic — see `zzop_summary`'s own crate doc for the drift class
//! this split exists to close. The MCP-specific surface (the stdio JSON-RPC loop, `tools/list` schemas,
//! `resources/*` handlers) is MCP-surface, not shared, and lives in the `zzop-mcp` package instead
//! (`packages/mcp/src`) — it depends on this crate for the shared dispatch + the embedded contract
//! table below.
//!
//! Module map:
//! - `tools`    — shared dispatch: the typed `analyze`/`analyze_envelope`/`cross_repo`/
//!   `check_endpoint`/`validate_envelope`/`validate_rule_pack` functions the `zzop` CLI's subcommands
//!   call directly, and the same functions the `zzop-mcp` package's `tools/call` dispatch calls (via
//!   `zzop-summary`) for its tool twins.
//! - `explain`  — `zzop explain <rule-id>`'s read-only lookup over the DSL rule data compiled into this
//!   binary. CLI-only (MCP already reaches the same data via the `rule-catalog` embedded contract
//!   resource), so it sits outside `tools`' MCP-mirrored dispatch — see its own module doc.
//! - `manifest` — `zzop manifest`/`zzop diff`'s structural-drift lane: the contract manifest of a
//!   cross-layer run and the delta between two of them. CLI-only, like `explain` (no `tools/call`
//!   twin — see its module doc for the judgment and `docs/contracts/surface-parity.json`'s
//!   `_cliOnlyLanes` for the recorded contract).
//! - `embedded` — compile-time embedded contract documents (`zzop://contract/<name>` over MCP,
//!   `zzop contract [<name>]` from a terminal) — the ONE table both surfaces resolve names through.
//! - `server`   — `version()` only: `CARGO_PKG_VERSION`, the workspace release SSOT, shared by the
//!   `zzop` CLI's `version` subcommand and the `zzop-mcp` package's `initialize`/`version` handlers so
//!   they can never disagree. The stdio protocol loop itself lives in `packages/mcp/src/server.rs`.
//!
//! The config front-end (`zzop.config.jsonc` discovery, JSONC, config→request mapping, `trees:
//! "auto"`) is NOT a module here — it lives in the shared `zzop-config` crate so both host products
//! map configs identically.

pub mod embedded;
pub mod explain;
pub mod manifest;
pub mod server;
pub mod tools;
