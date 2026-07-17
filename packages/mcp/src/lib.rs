//! zzop-mcp — the Node-free host: a standalone binary that runs the zzop engine with zero Node runtime,
//! for MCP clients (`.mcp.json` pointing at this executable) and for direct CLI use. Path B of the
//! mcp-distribution decision: the analysis engine, the bundled DSL packs (via the shared `zzop-config`
//! crate), and the authoring contracts all travel inside one self-contained executable.
//!
//! This crate is a THIN PROTOCOL FACADE: it extracts arguments from its wire formats (MCP `tools/call`
//! JSON, CLI argv) and calls the shared `zzop-summary` crate, which owns EVERY bit of summary/shaping/
//! filter/warning-merge logic. Nothing in this crate re-derives or forks that logic — see
//! `zzop_summary`'s own crate doc for the drift class this split exists to close.
//!
//! Module map:
//! - `server`    — stdio JSON-RPC 2.0 loop (initialize / tools/* / resources/*), silent-swallow-free.
//! - `tools`     — MCP tool definitions + dispatch (`analyze_repo`, `cross_repo`, ...): extract
//!   arguments, call `zzop_summary`, wrap the result into the MCP reply shape. Shared by the CLI
//!   subcommands.
//! - `resources` — MCP resources: the embedded authoring contracts (`zzop://contract/<name>`), so a
//!   custom-parser or rule author needs neither the zzop source repo nor Node.
//! - `embedded`  — compile-time embedded contract documents (the `resources` payload).
//!
//! The config front-end (`zzop.config.jsonc` discovery, JSONC, config→request mapping, `trees:
//! "auto"`) is NOT a module here — it lives in the shared `zzop-config` crate so a future full-CLI
//! binary maps configs identically.

pub mod embedded;
pub mod resources;
pub mod server;
pub mod tools;
