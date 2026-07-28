//! `zzop-mcp` — the MCP server product: a standalone binary that speaks MCP (JSON-RPC 2.0 over stdio)
//! against the shared zzop engine, with zero Node runtime. Path B of the mcp-distribution decision: the
//! analysis engine, the bundled DSL packs (via the shared `zzop-config` crate), and the authoring
//! contracts all travel inside one self-contained executable.
//!
//! This crate is a THIN PROTOCOL FACADE: it extracts arguments from the MCP `tools/call` JSON wire
//! format and calls the shared `zzop-summary` crate, which owns EVERY bit of summary/shaping/filter/
//! warning-merge logic. Nothing in this crate re-derives or forks that logic — see `zzop_summary`'s own
//! crate doc for the drift class this split exists to close. The `zzop` CLI (package `zzop-cli-bin`)
//! calls the identical `zzop_summary` functions from its own argv dispatch, so a CLI query and an MCP
//! tool call give the identical answer.
//!
//! Module map:
//! - `server`    — stdio JSON-RPC 2.0 loop (initialize / tools/* / resources/*), silent-swallow-free.
//!   `version()` itself is re-exported from the shared `zzop-summary` crate (`zzop_summary::version`,
//!   which reaches the single owner `zzop_facade::version`) so the CLI's `zzop version` and this
//!   server's `initialize`/`version` handlers can never disagree.
//! - `tools`     — MCP tool definitions (`tools/definitions.rs`) + dispatch (`analyze_repo`,
//!   `cross_repo`, ...): extract arguments, call `zzop_summary`, wrap the result into the MCP reply
//!   shape.
//! - `resources` — MCP resources: the embedded authoring contracts (`zzop://contract/<name>`, served
//!   from the shared `zzop_summary::contracts` table), so a custom-parser or rule author needs neither
//!   the zzop source repo nor Node.
//!
//! The config front-end (`zzop.config.jsonc` discovery, JSONC, config→request mapping, `trees:
//! "auto"`, and the request assembly that decides which trees a call is about) is NOT a module here —
//! it lives in the shared `zzop-config` crate so the CLI product maps configs identically. The
//! embedded contract documents themselves live in `zzop-summary` for the same reason, since the
//! `zzop contract [<name>]` CLI subcommand and this server's `resources/*` handlers must resolve the
//! exact same names to the exact same bytes.

pub mod resources;
pub mod server;
pub mod tools;
