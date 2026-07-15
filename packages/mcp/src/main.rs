//! `zzop-mcp` binary entry — thin argument dispatch over the library (`zzop_mcp::*`):
//!
//!   zzop-mcp analyze <path>       — analyze ONE repo/tree, print a JSON findings summary (Node-free).
//!   zzop-mcp cross <path>...      — analyze 2+ trees and print the cross-layer join (zzop's headline).
//!   zzop-mcp mcp                  — the MCP server over stdio (newline-delimited JSON-RPC 2.0).
//!
//! See `lib.rs` for the module map and the mcp-distribution decision doc for the host design.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("analyze") => {
            let Some(path) = args.get(2) else {
                eprintln!("usage: zzop-mcp analyze <path>");
                std::process::exit(2);
            };
            match zzop_mcp::tools::analyze(path) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("zzop-mcp: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some("cross") => {
            // `cross --config <path>` = config-first mode (the config's trees define the join);
            // `cross <path>...` = config-free paths mode. Mirrors the cross_repo tool's two modes.
            let (paths, config_path) = match args.get(2).map(String::as_str) {
                Some("--config") => match args.get(3) {
                    Some(cp) => (Vec::new(), Some(cp.as_str())),
                    None => {
                        eprintln!("usage: zzop-mcp cross --config <zzop.config.jsonc>");
                        std::process::exit(2);
                    }
                },
                _ => (args[2..].to_vec(), None),
            };
            match zzop_mcp::tools::cross_repo(&paths, config_path) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("zzop-mcp: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some("mcp") => zzop_mcp::server::run_stdio(),
        _ => {
            eprintln!(
                "usage: zzop-mcp <analyze <path> | cross <path>... | cross --config <path> | mcp>"
            );
            std::process::exit(2);
        }
    }
}
