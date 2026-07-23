//! `zzop` binary entry (package `zzop-cli-bin`) — the CLI: thin argument dispatch over the shared
//! `zzop-host` library (crates/host). The MCP server is the sibling product `zzop-mcp` (package
//! `zzop-mcp`, packages/mcp); both dispatch to the same `zzop_host::tools` handlers, so a CLI query and
//! an MCP tool call give the identical answer.
//!
//!   zzop analyze <path>              — analyze ONE repo/tree, print a JSON findings summary (Node-free).
//!   zzop analyze-envelope <file>     — Mode A: analyze a Normalized-AST envelope file in place of native parsing.
//!   zzop validate-envelope <file>    — offline "is this envelope well-formed?" report (exit 0 valid / 1 invalid).
//!   zzop validate-rule-pack <file>   — offline "does this DSL pack load + regexes compile?" report (exit 0 / 1).
//!   zzop cross <path>...             — analyze 2+ trees and print the cross-layer join (zzop's headline).
//!   zzop endpoint <pattern> <path>... — definitive "is io key X provided/consumed/joined?" query.
//!   zzop endpoint <pattern> --config <path> — same query, trees defined by a zzop.config.jsonc.
//!   zzop contract [<name>]           — list the embedded authoring contracts / print one to stdout.
//!   zzop version | --version         — print this binary's version (equals the MCP serverInfo.version).
//!   zzop help | --help | -h          — print the usage line plus one elaboration per subcommand (exit 0).
//!
//! See `zzop_host`'s own `lib.rs` for the shared module map and the mcp-distribution decision doc for
//! the host design. `cli.rs` (this crate) carries only this binary's own argv-parsing/usage helpers.

mod cli;

/// The one usage line — printed to stdout by `--help` (exit 0) and to stderr by every malformed
/// invocation (exit 2), so the two surfaces can never drift apart.
const USAGE: &str = "usage: zzop <analyze <path> | analyze-envelope <envelope.json> | validate-envelope <envelope.json> | validate-rule-pack <pack.json> | cross <path>... | cross --config <path> | endpoint <pattern> <path>... | endpoint <pattern> --config <path> | contract [<name>] | version>";

/// A one-line pointer at the bare-invocation/unknown-subcommand error path (exit 2): a bare `zzop` gives
/// no hint that `help` exists, or that MCP is the sibling `zzop-mcp` binary (not a `zzop` subcommand).
const BARE_INVOCATION_HINT: &str =
    "(run 'zzop help' for details; the MCP server is the 'zzop-mcp' binary)";

use cli::{reject_flag_like_args, run_file_validate};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("analyze") => {
            let Some(path) = args.get(2) else {
                eprintln!("usage: zzop analyze <path>");
                std::process::exit(2);
            };
            // Fixed arity: a trailing extra arg would otherwise be DROPPED silently — the user
            // believes it was analyzed (same never-silent rule as endpoint/contract's guards).
            if args.len() > 3 {
                eprintln!(
                    "usage: zzop analyze <path> (one path — got {})",
                    args.len() - 2
                );
                std::process::exit(2);
            }
            reject_flag_like_args([path.as_str()], "usage: zzop analyze <path>");
            match zzop_host::tools::analyze(path) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("zzop: {e}");
                    std::process::exit(1);
                }
            }
        }
        // Mode A: the file's content REPLACES native parsing entirely for this run (contrast
        // `analyze`, which walks a real tree) — same handler as the `analyze_envelope` MCP tool, so
        // this CLI form and a tool call give the identical answer.
        Some("analyze-envelope") => {
            let Some(path) = args.get(2) else {
                eprintln!("usage: zzop analyze-envelope <envelope.json>");
                std::process::exit(2);
            };
            if args.len() > 3 {
                eprintln!(
                    "usage: zzop analyze-envelope <envelope.json> (one file — got {})",
                    args.len() - 2
                );
                std::process::exit(2);
            }
            reject_flag_like_args(
                [path.as_str()],
                "usage: zzop analyze-envelope <envelope.json>",
            );
            let envelope_json = match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(e) => {
                    eprintln!("zzop: failed to read {path}: {e}");
                    std::process::exit(1);
                }
            };
            match zzop_host::tools::analyze_envelope(&envelope_json) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("zzop: {e}");
                    std::process::exit(1);
                }
            }
        }
        // Offline authoring checks — read a file, print a `{"valid":…,"issues":…}` report, exit by
        // validity. Same `zzop_summary` check the `validate_envelope`/`validate_rule_pack` MCP tools
        // call, so a CLI check and a tool call give the identical verdict.
        Some("validate-envelope") => run_file_validate(
            &args,
            "validate-envelope <envelope.json>",
            zzop_host::tools::validate_envelope,
        ),
        Some("validate-rule-pack") => run_file_validate(
            &args,
            "validate-rule-pack <pack.json>",
            zzop_host::tools::validate_rule_pack,
        ),
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
                                "usage: zzop cross --config <zzop.config.jsonc> (no extra paths — the config's trees define the join)"
                            );
                            std::process::exit(2);
                        }
                        (Vec::new(), Some(cp.as_str()))
                    }
                    None => {
                        eprintln!("usage: zzop cross --config <zzop.config.jsonc>");
                        std::process::exit(2);
                    }
                },
                _ => (args[2..].to_vec(), None),
            };
            // Paths mode needs 2+ paths — fewer is an arg-shape mistake (usage error, exit 2, same
            // as every other malformed invocation here), not a runtime failure. The handler keeps
            // its own "at least 2 paths" error for the MCP tool path, where exit codes don't exist.
            if config_path.is_none() && paths.len() < 2 {
                eprintln!("usage: zzop cross <path> <path>... (2+ paths) | cross --config <zzop.config.jsonc>");
                std::process::exit(2);
            }
            // Only the leading `--config` above is a recognized flag — a dash-shaped path (or a
            // misplaced `--config` inside the path list) is a usage error, never a path.
            reject_flag_like_args(
                paths.iter().map(String::as_str).chain(config_path),
                "usage: zzop cross <path> <path>... (2+ paths) | cross --config <zzop.config.jsonc>",
            );
            match zzop_host::tools::cross_repo(&paths, config_path) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("zzop: {e}");
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
                eprintln!("usage: zzop endpoint <pattern> <path>... | endpoint <pattern> --config <zzop.config.jsonc>");
                std::process::exit(2);
            };
            let (rest, config_path) = match args.get(3).map(String::as_str) {
                Some("--config") => match args.get(4) {
                    Some(cp) => (&args[5..], Some(cp.as_str())),
                    None => {
                        eprintln!("usage: zzop endpoint <pattern> --config <zzop.config.jsonc>");
                        std::process::exit(2);
                    }
                },
                _ => (&args[3..], None),
            };
            // Config mode takes no trailing paths (exactly ONE of path/paths/configPath — the
            // tool's own argument contract); paths mode needs 1+ paths. Either shape mistake is a
            // usage error (exit 2), same as every other malformed invocation here.
            if config_path.is_some() && !rest.is_empty() {
                eprintln!(
                    "usage: zzop endpoint <pattern> --config <zzop.config.jsonc> (no extra paths)"
                );
                std::process::exit(2);
            }
            if config_path.is_none() && rest.is_empty() {
                eprintln!("usage: zzop endpoint <pattern> <path>... | endpoint <pattern> --config <zzop.config.jsonc>");
                std::process::exit(2);
            }
            // The pattern and every path must be dash-free — `endpoint -x a b` is a usage error,
            // not a pattern query (only the positional `--config` above is a recognized flag).
            reject_flag_like_args(
                std::iter::once(pattern.as_str())
                    .chain(rest.iter().map(String::as_str))
                    .chain(config_path),
                "usage: zzop endpoint <pattern> <path>... | endpoint <pattern> --config <zzop.config.jsonc>",
            );
            let result = match (config_path, rest.len()) {
                (Some(_), _) => zzop_host::tools::check_endpoint(pattern, None, &[], config_path),
                (None, 1) => zzop_host::tools::check_endpoint(pattern, Some(&rest[0]), &[], None),
                (None, _) => zzop_host::tools::check_endpoint(pattern, None, rest, None),
            };
            match result {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("zzop: {e}");
                    std::process::exit(1);
                }
            }
        }
        // The embedded authoring contracts from a terminal — the same documents MCP `resources/read`
        // serves via the same `embedded::find` lookup (no drift). No name lists them; a name prints that
        // document's exact embedded bytes (raw, pipe-safe — `contract config-surface | jq` is byte-identical).
        Some("contract") => match args.get(2) {
            None => {
                for doc in zzop_host::embedded::CONTRACT_DOCS {
                    println!("{}  [{}]  {}", doc.name, doc.mime, doc.description);
                }
            }
            Some(_) if args.len() > 3 => {
                eprintln!("usage: zzop contract [<name>]");
                std::process::exit(2);
            }
            Some(name) => {
                reject_flag_like_args([name.as_str()], "usage: zzop contract [<name>]");
                match zzop_host::embedded::find(name) {
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
                        let known: Vec<&str> = zzop_host::embedded::names().collect();
                        eprintln!(
                            "zzop: unknown contract {name:?} — known contracts: {}",
                            known.join(", ")
                        );
                        std::process::exit(1);
                    }
                }
            }
        },
        // The version surface: `server::version()` = `CARGO_PKG_VERSION`, the workspace release version,
        // shared with the `zzop-mcp` binary and MCP `initialize`, so all three can never disagree.
        Some("version") | Some("--version") => {
            println!("zzop {}", zzop_host::server::version());
        }
        // The polite lane: an explicit help REQUEST prints the usage line + one elaboration per
        // subcommand to stdout, exit 0. The exit-2 stderr lane below stays a bare usage line +
        // `BARE_INVOCATION_HINT` — an error is a pointer AT `help`, not a tutorial.
        Some("help") | Some("--help") | Some("-h") => {
            println!("{USAGE}");
            println!("  analyze <path> — analyze ONE repo/tree, print a JSON findings summary");
            println!(
                "  analyze-envelope <envelope.json> — Mode A: a Normalized-AST envelope file REPLACES native parsing, print the same JSON findings summary"
            );
            println!(
                "  validate-envelope <envelope.json> — offline: is this envelope well-formed? print {{valid,issues}}, exit 0 valid / 1 invalid"
            );
            println!(
                "  validate-rule-pack <pack.json> — offline: does this DSL rule pack load + every matcher regex compile? print {{valid,issues}}, exit 0/1"
            );
            println!("  cross <path>... | cross --config <path> — analyze 2+ trees, print the cross-layer join");
            println!(
                "  endpoint <pattern> <path>... | endpoint <pattern> --config <path> — definitive \"is io key X provided/consumed/joined?\" query"
            );
            println!(
                "  contract [<name>] — no args lists the embedded doc resources; `contract <name>` prints one"
            );
            println!("  version — print this binary's version (equals the MCP serverInfo.version)");
            println!(
                "  (the MCP server is the sibling 'zzop-mcp' binary — it speaks JSON-RPC over stdio, not a 'zzop' subcommand)"
            );
        }
        _ => {
            eprintln!("{USAGE}");
            eprintln!("{BARE_INVOCATION_HINT}");
            std::process::exit(2);
        }
    }
}
