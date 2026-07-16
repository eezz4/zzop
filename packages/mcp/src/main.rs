//! `zzop-mcp` binary entry — thin argument dispatch over the library (`zzop_mcp::*`):
//!
//!   zzop-mcp analyze <path>            — analyze ONE repo/tree, print a JSON findings summary (Node-free).
//!   zzop-mcp cross <path>...           — analyze 2+ trees and print the cross-layer join (zzop's headline).
//!   zzop-mcp endpoint <pattern> <path>... — definitive "is io key X provided/consumed/joined?" query.
//!   zzop-mcp endpoint <pattern> --config <path> — same query, trees defined by a zzop.config.jsonc.
//!   zzop-mcp contract [<name>]         — list the embedded authoring contracts / print one to stdout.
//!   zzop-mcp version | --version       — print this binary's version (the MCP serverInfo.version value).
//!   zzop-mcp mcp                       — the MCP server over stdio (newline-delimited JSON-RPC 2.0).
//!   zzop-mcp help | --help | -h        — print the usage line (exit 0).
//!
//! See `lib.rs` for the module map and the mcp-distribution decision doc for the host design.

/// The one usage line — printed to stdout by `--help` (exit 0) and to stderr by every malformed
/// invocation (exit 2), so the two surfaces can never drift apart.
const USAGE: &str = "usage: zzop-mcp <analyze <path> | cross <path>... | cross --config <path> | endpoint <pattern> <path>... | endpoint <pattern> --config <path> | contract [<name>] | version | mcp>";

/// A dash-leading argument in a path/pattern position is NEVER swallowed as a path or pattern —
/// `zzop-mcp analyze --help` must be a usage error, not "path does not exist: --help". The only
/// flags this binary recognizes are parsed positionally before this check runs (`--config` for
/// cross/endpoint, top-level `--help`/`--version`); anything else dash-shaped exits 2 with the
/// subcommand's usage line.
fn reject_flag_like_args<'a>(args: impl IntoIterator<Item = &'a str>, usage: &str) {
    for arg in args {
        if arg.starts_with('-') {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("analyze") => {
            let Some(path) = args.get(2) else {
                eprintln!("usage: zzop-mcp analyze <path>");
                std::process::exit(2);
            };
            // Fixed arity: a trailing extra arg would otherwise be DROPPED silently — the user
            // believes it was analyzed (same never-silent rule as endpoint/contract's guards).
            if args.len() > 3 {
                eprintln!(
                    "usage: zzop-mcp analyze <path> (one path — got {})",
                    args.len() - 2
                );
                std::process::exit(2);
            }
            reject_flag_like_args([path.as_str()], "usage: zzop-mcp analyze <path>");
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
                    Some(cp) => {
                        // Config mode is fixed-arity: a trailing path after the config file would
                        // be DROPPED silently (the user believes it joined the analysis) — the
                        // config's trees alone define the join.
                        if args.len() > 4 {
                            eprintln!(
                                "usage: zzop-mcp cross --config <zzop.config.jsonc> (no extra paths — the config's trees define the join)"
                            );
                            std::process::exit(2);
                        }
                        (Vec::new(), Some(cp.as_str()))
                    }
                    None => {
                        eprintln!("usage: zzop-mcp cross --config <zzop.config.jsonc>");
                        std::process::exit(2);
                    }
                },
                _ => (args[2..].to_vec(), None),
            };
            // Paths mode needs 2+ paths — fewer is an arg-shape mistake (usage error, exit 2, same
            // as every other malformed invocation here), not a runtime failure. The handler keeps
            // its own "at least 2 paths" error for the MCP tool path, where exit codes don't exist.
            if config_path.is_none() && paths.len() < 2 {
                eprintln!("usage: zzop-mcp cross <path> <path>... (2+ paths) | cross --config <zzop.config.jsonc>");
                std::process::exit(2);
            }
            // Only the leading `--config` above is a recognized flag — a dash-shaped path (or a
            // misplaced `--config` inside the path list) is a usage error, never a path.
            reject_flag_like_args(
                paths.iter().map(String::as_str).chain(config_path),
                "usage: zzop-mcp cross <path> <path>... (2+ paths) | cross --config <zzop.config.jsonc>",
            );
            match zzop_mcp::tools::cross_repo(&paths, config_path) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("zzop-mcp: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some("endpoint") => {
            // `endpoint <pattern> <path>...` — one path = single-tree mode (the check_endpoint
            // tool's `path` argument), 2+ = config-free paths mode (`paths`);
            // `endpoint <pattern> --config <path>` = config-first mode (the tool's `configPath`
            // argument), parsed exactly like `cross --config` above. Same handler as the MCP tool,
            // so a CLI query and a tool call give the identical answer.
            let Some(pattern) = args.get(2) else {
                eprintln!("usage: zzop-mcp endpoint <pattern> <path>... | endpoint <pattern> --config <zzop.config.jsonc>");
                std::process::exit(2);
            };
            let (rest, config_path) = match args.get(3).map(String::as_str) {
                Some("--config") => match args.get(4) {
                    Some(cp) => (&args[5..], Some(cp.as_str())),
                    None => {
                        eprintln!(
                            "usage: zzop-mcp endpoint <pattern> --config <zzop.config.jsonc>"
                        );
                        std::process::exit(2);
                    }
                },
                _ => (&args[3..], None),
            };
            // Config mode takes no trailing paths (exactly ONE of path/paths/configPath — the
            // tool's own argument contract); paths mode needs 1+ paths. Either shape mistake is a
            // usage error (exit 2), same as every other malformed invocation here.
            if config_path.is_some() && !rest.is_empty() {
                eprintln!("usage: zzop-mcp endpoint <pattern> --config <zzop.config.jsonc> (no extra paths)");
                std::process::exit(2);
            }
            if config_path.is_none() && rest.is_empty() {
                eprintln!("usage: zzop-mcp endpoint <pattern> <path>... | endpoint <pattern> --config <zzop.config.jsonc>");
                std::process::exit(2);
            }
            // The pattern and every path must be dash-free — `endpoint -x a b` is a usage error,
            // not a pattern query (only the positional `--config` above is a recognized flag).
            reject_flag_like_args(
                std::iter::once(pattern.as_str())
                    .chain(rest.iter().map(String::as_str))
                    .chain(config_path),
                "usage: zzop-mcp endpoint <pattern> <path>... | endpoint <pattern> --config <zzop.config.jsonc>",
            );
            let result = match (config_path, rest.len()) {
                (Some(_), _) => zzop_mcp::tools::check_endpoint(pattern, None, &[], config_path),
                (None, 1) => zzop_mcp::tools::check_endpoint(pattern, Some(&rest[0]), &[], None),
                (None, _) => zzop_mcp::tools::check_endpoint(pattern, None, rest, None),
            };
            match result {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("zzop-mcp: {e}");
                    std::process::exit(1);
                }
            }
        }
        // The embedded authoring contracts, reachable from a terminal — the same nine documents MCP
        // `resources/read` serves, resolved through the same `embedded::find` lookup, so the two
        // surfaces cannot drift. No name lists them (name + one-line description + mime, one per
        // line); a name prints that document's exact embedded bytes (raw, pipe-safe — no trailing
        // newline added, so `zzop-mcp contract config-surface | jq` sees the byte-identical file).
        Some("contract") => match args.get(2) {
            None => {
                for doc in zzop_mcp::embedded::CONTRACT_DOCS {
                    println!("{}  [{}]  {}", doc.name, doc.mime, doc.description);
                }
            }
            Some(_) if args.len() > 3 => {
                eprintln!("usage: zzop-mcp contract [<name>]");
                std::process::exit(2);
            }
            Some(name) => {
                reject_flag_like_args([name.as_str()], "usage: zzop-mcp contract [<name>]");
                match zzop_mcp::embedded::find(name) {
                    Some(doc) => {
                        use std::io::Write;
                        std::io::stdout()
                            .write_all(doc.content.as_bytes())
                            .expect("write contract document to stdout");
                    }
                    None => {
                        // An unknown NAME is a runtime lookup failure (exit 1, like an unknown
                        // resource URI over MCP), not an argument-shape mistake (exit 2) — and the
                        // error names every valid choice, so the caller never has to guess.
                        let known: Vec<&str> = zzop_mcp::embedded::names().collect();
                        eprintln!(
                            "zzop-mcp: unknown contract {name:?} — known contracts: {}",
                            known.join(", ")
                        );
                        std::process::exit(1);
                    }
                }
            }
        },
        // The one version surface the binary has (`serverInfo.version` aside): release builds print
        // the tag-stamped release version, dev builds the 0.0.0 workspace placeholder — the exact
        // `server::version()` value, so the CLI and MCP `initialize` can never disagree.
        Some("version") | Some("--version") => {
            println!("zzop-mcp {}", zzop_mcp::server::version());
        }
        Some("mcp") => zzop_mcp::server::run_stdio(),
        // The polite lane: an explicit help REQUEST prints the usage line to stdout and exits 0 —
        // only an invocation the dispatch cannot honor is the exit-2 stderr lane below.
        Some("help") | Some("--help") | Some("-h") => println!("{USAGE}"),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}
