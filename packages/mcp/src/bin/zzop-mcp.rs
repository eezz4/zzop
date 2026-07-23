//! `zzop-mcp` binary entry (package `zzop-mcp`) — the MCP server over stdio (newline-delimited
//! JSON-RPC 2.0). Thin shim over this crate's own library (`zzop_mcp::server`); the CLI subcommands
//! (`analyze`/`cross`/`endpoint`/…) live in the sibling product `zzop` (package `zzop-cli-bin`,
//! `packages/cli-bin/src/main.rs`), both dispatching to the shared `zzop_host`/`zzop_summary` handlers so
//! a tool call and a CLI query give the identical answer.
//!
//!   zzop-mcp            — serve MCP over stdio (the bare form MCP clients register).
//!   zzop-mcp mcp        — same; the explicit subcommand `.mcp.json` / the MCPB manifest use in `args`.
//!   zzop-mcp version | --version — print this binary's version (equals the MCP serverInfo.version).
//!   zzop-mcp help | --help | -h  — print the usage line (exit 0).
//!
//! See `lib.rs` for the module map and the mcp-distribution decision doc for the host design.

const USAGE: &str =
    "usage: zzop-mcp [mcp]  — serve MCP over stdio (JSON-RPC 2.0). Run the 'zzop' binary for CLI subcommands.";

fn main() {
    match std::env::args().nth(1).as_deref() {
        // The registered form (`.mcp.json` / MCPB manifest pass `args: ["mcp"]`) AND the bare form both
        // serve — a plain `zzop-mcp` on PATH is the server, no subcommand needed.
        None | Some("mcp") => zzop_mcp::server::run_stdio(),
        Some("version") | Some("--version") => {
            println!("zzop-mcp {}", zzop_mcp::server::version());
        }
        Some("help") | Some("--help") | Some("-h") => println!("{USAGE}"),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}
