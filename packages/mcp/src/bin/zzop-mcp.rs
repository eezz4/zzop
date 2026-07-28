//! `zzop-mcp` binary entry (package `zzop-mcp`) — the MCP server over stdio (newline-delimited
//! JSON-RPC 2.0). Thin shim over this crate's own library (`zzop_mcp::server`); the CLI subcommands
//! (`analyze`/`cross`/`endpoint`/…) live in the sibling product `zzop` (package `zzop-cli-bin`,
//! `packages/cli-bin/src/main.rs`), both calling the same shared `zzop_summary` functions so
//! a tool call and a CLI query give the identical answer.
//!
//!   zzop-mcp            — serve MCP over stdio (the bare form MCP clients register).
//!   zzop-mcp mcp        — same; the explicit subcommand a client's own `.mcp.json`, the Claude Code
//!                          plugin's `plugin.json` `mcpServers`, and the MCPB manifest all use in `args`.
//!   zzop-mcp version [--verbose] — print this binary's version (equals the MCP serverInfo.version);
//!                          `--verbose` adds every parser's fingerprint (same string as `zzop version --verbose`).
//!   zzop-mcp help | --help | -h  — print the usage line (exit 0).
//!
//! See `lib.rs` for the module map and the mcp-distribution decision doc for the host design.

const USAGE: &str =
    "usage: zzop-mcp [mcp | version [--verbose] | help]  — serve MCP over stdio (JSON-RPC 2.0). Run the 'zzop' binary for CLI subcommands.";

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
        // The bare version stays the default (a one-token line scripts parse); `--verbose` adds every
        // parser's fingerprint — the same string `zzop version --verbose` prints, from the same owner,
        // so the fingerprint is reachable from BOTH products rather than the CLI alone.
        Some("version") | Some("--version") => match std::env::args().nth(2).as_deref() {
            None => println!("zzop-mcp {}", zzop_mcp::server::version()),
            // A help request is ANSWERED, never turned into an error — the same contract the `zzop`
            // CLI's per-subcommand help gate enforces. Without this arm the two hosts disagreed about
            // what `version --help` means (0 and a usage line vs 2 and an error), so a wrapper probing
            // which binary it holds read `zzop-mcp` as broken.
            Some("-h") | Some("--help") => println!("{USAGE}"),
            Some("--verbose") if std::env::args().count() == 3 => {
                println!("{}", zzop_mcp::server::version_string());
            }
            // Names the first argument that is NOT `--verbose`: the arm above falls through here on
            // `version --verbose extra`, and reporting `"--verbose"` would name the correct one.
            Some(_) => {
                let args: Vec<String> = std::env::args().skip(2).collect();
                let bad = args.iter().find(|a| *a != "--verbose");
                eprintln!("usage: zzop-mcp version [--verbose] (unexpected argument {bad:?})");
                std::process::exit(2);
            }
        },
        Some("help") | Some("--help") | Some("-h") => println!("{USAGE}"),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}
