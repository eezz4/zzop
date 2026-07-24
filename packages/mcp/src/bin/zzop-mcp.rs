//! `zzop-mcp` binary entry (package `zzop-mcp`) — the MCP server over stdio (newline-delimited
//! JSON-RPC 2.0). Thin shim over this crate's own library (`zzop_mcp::server`); the CLI subcommands
//! (`analyze`/`cross`/`endpoint`/…) live in the sibling product `zzop` (package `zzop-cli-bin`,
//! `packages/cli-bin/src/main.rs`), both dispatching to the shared `zzop_host`/`zzop_summary` handlers so
//! a tool call and a CLI query give the identical answer.
//!
//!   zzop-mcp            — serve MCP over stdio (the bare form MCP clients register).
//!   zzop-mcp mcp        — same; the explicit subcommand a client's own `.mcp.json`, the Claude Code
//!                          plugin's `plugin.json` `mcpServers`, and the MCPB manifest all use in `args`.
//!   zzop-mcp version | --version — print this binary's version (equals the MCP serverInfo.version).
//!   zzop-mcp help | --help | -h  — print the usage line (exit 0).
//!
//! See `lib.rs` for the module map and the mcp-distribution decision doc for the host design.

const USAGE: &str =
    "usage: zzop-mcp [mcp]  — serve MCP over stdio (JSON-RPC 2.0). Run the 'zzop' binary for CLI subcommands.";

fn main() {
    match std::env::args().nth(1).as_deref() {
        // The registered form (a client's own `.mcp.json` / the plugin's `plugin.json` `mcpServers` /
        // the MCPB manifest all pass `args: ["mcp"]`) AND the bare form both serve — a plain `zzop-mcp`
        // on PATH is the server, no subcommand needed.
        None | Some("mcp") => {
            // Announce which build is actually serving, on stderr (stdout is the JSON-RPC channel and
            // must carry nothing else). An MCP client shows stderr in its server log, so "which zzop is
            // running?" stops being a question the operator has to answer by hunting the filesystem —
            // the 2026-07-24 dogfooding session lost time to exactly that, with a 0.18.0 binary on PATH
            // while every manifest in the repo said 0.22.0. This is the binary's own compiled-in
            // version, NOT a check against the latest release: the binary stays network-free by
            // decision (see the mcp-distribution doc), so "is there a newer one?" belongs to the
            // delivery layer, never here.
            eprintln!(
                "zzop-mcp {} — serving MCP over stdio. Newer releases: https://github.com/eezz4/zzop/releases",
                zzop_mcp::server::version()
            );
            zzop_mcp::server::run_stdio()
        }
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
