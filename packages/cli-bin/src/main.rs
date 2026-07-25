//! `zzop` binary entry (package `zzop-cli-bin`) — the CLI: thin argument dispatch over the shared
//! `zzop-host` library (crates/host). The MCP server is the sibling product `zzop-mcp` (package
//! `zzop-mcp`, packages/mcp); both dispatch to the same `zzop_host::tools` handlers, so a CLI query and
//! an MCP tool call give the identical answer.
//!
//!   zzop analyze <path>              — analyze ONE repo/tree, print a JSON findings summary (Node-free).
//!   zzop analyze-envelope <file>     — Mode A: analyze a Normalized-AST envelope file in place of native parsing.
//!   zzop validate-envelope <file>    — offline "is this envelope well-formed?" report (exit 0 valid / 1 invalid).
//!   zzop validate-rule-pack <file>   — offline "does this DSL pack load, and can every rule fire?" report (exit 0 / 1).
//!   zzop cross <path>...             — analyze 2+ trees and print the cross-layer join (zzop's headline).
//!   zzop endpoint <pattern> <path>... — definitive "is io key X provided/consumed/joined?" query.
//!   zzop endpoint <pattern> --config <path> — same query, trees defined by a zzop.config.jsonc.
//!   zzop manifest <path>...          — the run's structural contract manifest (identity only) — commit it, diff a later run against it.
//!   zzop diff <a.json> <b.json>      — the delta between two manifests: bucket transitions first.
//!   zzop contract [<name>]           — list the embedded authoring contracts / print one to stdout.
//!   zzop explain <rule-id>           — print one bundled DSL rule's compiled-in data to stdout.
//!   zzop version | --version         — print this binary's version (equals the MCP serverInfo.version).
//!   zzop help | --help | -h          — print the usage line plus one elaboration per subcommand (exit 0).
//!
//! See `zzop_host`'s own `lib.rs` for the shared module map and the mcp-distribution decision doc for
//! the host design. `cli.rs` (this crate) carries only this binary's own argv-parsing/usage helpers —
//! including the usage line and the `help` elaboration, which sit next to the parsers that print them.

mod cli;

/// The one usage line — printed to stdout by `--help` (exit 0) and to stderr by every malformed
/// invocation (exit 2), so the two surfaces can never drift apart. Stays in THIS file: the `zzop-mcp`
/// package's `surface_prose` meta-test reads this literal out of `packages/cli-bin/src/main.rs` by
/// path, to pin that every MCP tool's CLI twin subcommand is named here.
const USAGE: &str = "usage: zzop <analyze <path> | analyze-envelope <envelope.json> | validate-envelope <envelope.json> | validate-rule-pack <pack.json> | cross <path>... | cross --config <path> | endpoint <pattern> <path>... | endpoint <pattern> --config <path> | manifest <path>... | manifest --config <path> | diff <a.json> <b.json> | contract [<name>] | explain <rule-id> | version>";

/// A one-line pointer at the bare-invocation/unknown-subcommand error path (exit 2): a bare `zzop` gives
/// no hint that `help` exists, or that MCP is the sibling `zzop-mcp` binary (not a `zzop` subcommand).
const BARE_INVOCATION_HINT: &str =
    "(run 'zzop help' for details; the MCP server is the 'zzop-mcp' binary)";

use cli::{
    parse_trees_args, print_help, read_or_exit, reject_flag_like_args, run_diff, run_file_validate,
    run_lookup,
};

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
            print_result(zzop_host::tools::analyze(path));
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
            let envelope_json = read_or_exit(path);
            print_result(zzop_host::tools::analyze_envelope(&envelope_json));
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
        // `cross --config <path>` = config-first mode (the config's trees define the join);
        // `cross <path>...` = config-free paths mode. Mirrors the cross_repo tool's two modes.
        Some("cross") => {
            let (paths, config_path) = parse_trees_args(&args, "cross");
            print_result(zzop_host::tools::cross_repo(&paths, config_path));
        }
        // The structural-drift lane: the SAME two source modes as `cross` (one shared argv parser),
        // projecting the same analysis into identity rows instead of a capped summary. CLI-only —
        // no MCP tool twin (see `zzop_host::manifest`'s module doc for the judgment).
        Some("manifest") => {
            let (paths, config_path) = parse_trees_args(&args, "manifest");
            print_result(zzop_host::manifest::manifest(&paths, config_path));
        }
        // Pure post-processing over two already-produced manifests — no analysis, no tree walk.
        Some("diff") => run_diff(&args),
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
            print_result(result);
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
        // Read-only lookup over the DSL rule data compiled INTO this binary (`zzop_host::explain`, never
        // `docs/rules/catalog.md` prose — see that module's doc). Fixed arity (unlike `contract`'s
        // optional name): a rule id is always required. Same two-lane contract as `contract`: a
        // dash-shaped/missing/extra id is a usage error (exit 2, `run_lookup`); a real-but-unexplainable
        // id (unknown, ambiguous, a whole pack, or a native analysis id) is a runtime lookup failure
        // (exit 1, `zzop_host::explain::explain`'s own `Err` message).
        Some("explain") => run_lookup(&args, "explain <rule-id>", zzop_host::explain::explain),
        // The version surface: `server::version()` = `CARGO_PKG_VERSION`, the workspace release version,
        // shared with the `zzop-mcp` binary and MCP `initialize`, so all three can never disagree.
        Some("version") | Some("--version") => {
            println!("zzop {}", zzop_host::server::version());
        }
        Some("help") | Some("--help") | Some("-h") => print_help(),
        _ => {
            eprintln!("{USAGE}");
            eprintln!("{BARE_INVOCATION_HINT}");
            std::process::exit(2);
        }
    }
}

/// Every handler's terminal step: `Ok` to stdout, `Err` as `zzop: <message>` to stderr + exit 1.
/// (`cli.rs`'s own `print_or_exit` is the diverging-return twin for the `run_*` helpers, which never
/// return to this match at all.)
fn print_result(result: Result<String, String>) {
    match result {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("zzop: {e}");
            std::process::exit(1);
        }
    }
}
