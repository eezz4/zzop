//! `zzop` binary entry (package `zzop-cli-bin`) — the CLI: thin argument dispatch straight onto the
//! shared `zzop-summary` library (crates/summary). The MCP server is the sibling product `zzop-mcp`
//! (package `zzop-mcp`, packages/mcp); both call the same `zzop_summary` functions, so a CLI query and
//! an MCP tool call give the identical answer.
//!
//!   zzop analyze <path>              — analyze ONE repo/tree, print a JSON findings summary (Node-free).
//!   zzop analyze --config <path>     — same analysis, tree named by a zzop.config.jsonc at any location.
//!   zzop analyze-envelope <file>     — Mode A: analyze a Normalized-AST envelope file in place of native parsing.
//!   zzop validate-envelope <file>    — offline "is this envelope well-formed?" report (exit 0 valid / 1 invalid).
//!   zzop validate-rule-pack <file>   — offline "does this DSL pack load, and can every rule fire?" report (exit 0 / 1).
//!   zzop cross <path>...             — analyze 2+ trees and print the cross-layer join (zzop's headline).
//!   zzop endpoint <pattern> <path>... — definitive "is io key X provided/consumed/joined?" query.
//!   zzop endpoint <pattern> --config <path> — same query, trees defined by a zzop.config.jsonc.
//!   zzop manifest <path>...          — the run's structural contract manifest (identity only) — commit it, diff a later run against it.
//!   zzop diff <a.json> <b.json>      — the delta between two manifests: bucket transitions first.
//!   zzop facts <path>...             — the run's post-assembly facts (per-tree CommonIr + the whole cross-layer join, uncapped) for your own rule program.
//!   zzop file <path> <tree>...       — what does zzop know about THIS FILE: its tree, its verdict (analyzed / lexical-only / degraded / not-found), symbols, io, both edge directions, findings.
//!   zzop graph <path>...             — a picture for an EXTERNAL renderer, never drawn here. `--format mermaid` (default) serializes any `--domain` as a flowchart, scoped (`--top`, `--scope`) with every cap disclosed in the document; `--format cosmograph-nodes|cosmograph-links` serializes `--domain dep` as UNCAPPED NDJSON tables for an interactive viewer, with the census on stderr so stdout stays a parseable table.
//!   zzop init [--force]              — write the embedded starter zzop.config.jsonc into the current directory.
//!   zzop contract [<name>]           — list the embedded authoring contracts / print one to stdout.
//!   zzop explain <rule-id>           — print one bundled DSL rule's compiled-in data to stdout.
//!   zzop version [--verbose]         — print this binary's version (equals the MCP serverInfo.version); `--verbose` adds every parser's fingerprint.
//!   zzop <subcommand> --help | -h    — print that one subcommand's own elaboration (exit 0).
//!   zzop help | --help | -h          — print the usage line plus one elaboration per subcommand (exit 0).
//!
//! The three analysis lanes (`analyze`, `analyze-envelope`, `cross`) additionally take the findings-view
//! knobs `--severity <critical|warning|info>` / `--rule <id>` / `--limit <n>` — the argv spelling of the
//! same three arguments their MCP tool twins take.
//!
//! See `zzop_summary`'s own `lib.rs` for the shared module map and the mcp-distribution decision doc for
//! the product design. The `cli/` module (this crate) carries only this binary's own argv-parsing/usage
//! helpers, split by responsibility: `cli/args.rs` (argv shape), `cli/help.rs` (the help elaboration),
//! `cli/run.rs` (the diverging subcommand runners), `cli/analysis.rs` (the four analysis lanes).

mod cli;

/// The one usage line — printed to stdout by `--help` (exit 0) and to stderr by every malformed
/// invocation (exit 2), so the two surfaces can never drift apart. Stays in THIS file: the `zzop-mcp`
/// package's `surface_prose` meta-test reads this literal out of `packages/cli-bin/src/main.rs` by
/// path, to pin that every MCP tool's CLI twin subcommand is named here.
const USAGE: &str = "usage: zzop <analyze <path> | analyze --config <path> | analyze-envelope <envelope.json> | validate-envelope <envelope.json> | validate-rule-pack <pack.json> | cross <path>... | cross --config <path> | file <path> <tree>... | file <path> --config <path> | endpoint <pattern> <path>... | endpoint <pattern> --config <path> | manifest <path>... | manifest --config <path> | diff <a.json> <b.json> | facts <path>... | facts --config <path> | graph <path>... | graph --config <path> [--domain <join|dep|risk|posture>] [--format <mermaid|cosmograph-nodes|cosmograph-links>] [--scope <prefix>] [--top <n>] | init [--force] | contract [<name>] | explain <rule-id> | version [--verbose]> (analyze, analyze-envelope and cross also take [--severity <critical|warning|info>] [--rule <id>] [--limit <n>]; every subcommand takes --help)";

/// A one-line pointer at the bare-invocation/unknown-subcommand error path (exit 2): a bare `zzop` gives
/// no hint that `help` exists, or that MCP is the sibling `zzop-mcp` binary (not a `zzop` subcommand).
const BARE_INVOCATION_HINT: &str =
    "(run 'zzop help' for details; the MCP server is the 'zzop-mcp' binary)";

use cli::analysis::{run_analyze, run_analyze_envelope, run_cross, run_endpoint, run_file};
use cli::{
    parse_trees_args, print_help, reject_flag_like_args, run_diff, run_file_validate, run_graph,
    run_init, run_lookup,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // A help REQUEST is answered before anything else parses argv — `zzop analyze --help` used to hit
    // `reject_flag_like_args` and exit 2 to stderr, which hands an error to the one caller who asked
    // for help. One gate rather than a copy in every branch: the branches below are what would drift.
    cli::help::handle_subcommand_help(&args);
    match args.get(1).map(String::as_str) {
        Some("analyze") => run_analyze(&args),
        // Mode A: the file's content REPLACES native parsing entirely for this run (contrast
        // `analyze`, which walks a real tree) — same handler as the `analyze_envelope` MCP tool, so
        // this CLI form and a tool call give the identical answer.
        Some("analyze-envelope") => run_analyze_envelope(&args),
        // Offline authoring checks — read a file, print a verdict report, exit by validity. Same
        // `zzop_summary` check the `validate_envelope`/`validate_rule_pack` MCP tools call, so a CLI
        // check and a tool call give the identical verdict. The two reports are NOT the same shape:
        // `validate-envelope` prints `{"valid":…,"issues":…,"hints":…}`, where `hints` is an advisory
        // second axis that never moves `valid` or the exit code; `validate-rule-pack` prints
        // `{"valid":…,"issues":…}` with no `hints` key at all — deliberately, since rule packs have no
        // hint pass and an always-empty array would claim a search that never ran.
        Some("validate-envelope") => run_file_validate(
            &args,
            "validate-envelope <envelope.json>",
            zzop_summary::validate_envelope_only_json,
        ),
        Some("validate-rule-pack") => run_file_validate(
            &args,
            "validate-rule-pack <pack.json>",
            zzop_summary::validate_rule_pack_json,
        ),
        // `cross --config <path>` = config-first mode (the config's trees define the join);
        // `cross <path>...` = config-free paths mode. Mirrors the cross_repo tool's two modes.
        Some("cross") => run_cross(&args),
        // The custom-rule extension point's EMIT half: the same analysis as `cross`, projected as the
        // uncapped post-assembly fact substrate for a user's own rule program instead of a capped
        // summary. Takes ONE path too (unlike `cross`/`manifest`) — the join is meaningful over a
        // single tree, so a rule author does not have to invent a second one. CLI-only — no MCP tool
        // twin (see `zzop_summary::facts`'s module doc for the judgment, and
        // `docs/contracts/surface-parity.json`'s `_cliOnlyLanes` for the recorded contract).
        Some("facts") => {
            let (paths, config_path) = parse_trees_args(&args, "facts", 1);
            print_result(zzop_summary::facts_json(&paths, config_path));
        }
        // The one lane whose product is not JSON: the same analysis, serialized as MERMAID text for an
        // external renderer to draw (zzop renders no pixels). Takes ONE path too, like `facts`.
        // Scoped by construction (`--top` per bucket, `--scope` prefix) with every cap and filter
        // disclosed inside the emitted document. CLI-only — no MCP tool twin (that judgment is recorded
        // in `docs/contracts/surface-parity.json`'s `_cliOnlyLanes`; `zzop_summary::graph`'s module doc
        // answers the neighbouring question, why the product is mermaid text at all).
        Some("graph") => run_graph(&args),
        // The structural-drift lane: the SAME two source modes as `cross` (one shared argv parser),
        // projecting the same analysis into identity rows instead of a capped summary. CLI-only —
        // no MCP tool twin (that judgment is recorded in `docs/contracts/surface-parity.json`'s
        // `_cliOnlyLanes`; `zzop_summary::manifest`'s module doc answers the neighbouring question, why
        // an identity-only lane exists next to the capped summary).
        Some("manifest") => {
            let (paths, config_path) = parse_trees_args(&args, "manifest", 2);
            print_result(zzop_summary::manifest_json(&paths, config_path));
        }
        // Pure post-processing over two already-produced manifests — no analysis, no tree walk.
        Some("diff") => run_diff(&args),
        Some("endpoint") => run_endpoint(&args),
        Some("file") => run_file(&args),
        // The one write this binary performs: the embedded starter-config document, dropped into the
        // current directory as a file. Not a generator — the bytes are the same `config-template`
        // contract `contract`/`resources/read` serve below, so the three surfaces cannot disagree.
        Some("init") => run_init(&args),
        // The embedded authoring contracts from a terminal — the same documents MCP `resources/read`
        // serves via the same `embedded::find` lookup (no drift). No name lists them; a name prints that
        // document's exact embedded bytes (raw, pipe-safe — `contract config-surface | jq` is byte-identical).
        Some("contract") => match args.get(2) {
            None => {
                for doc in zzop_summary::contracts::CONTRACT_DOCS {
                    println!("{}  [{}]  {}", doc.name, doc.mime, doc.description);
                }
            }
            Some(_) if args.len() > 3 => {
                eprintln!("usage: zzop contract [<name>]");
                std::process::exit(2);
            }
            Some(name) => {
                reject_flag_like_args([name.as_str()], "usage: zzop contract [<name>]");
                match zzop_summary::contracts::find(name) {
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
                        let known: Vec<&str> = zzop_summary::contracts::names().collect();
                        eprintln!(
                            "zzop: unknown contract {name:?} — known contracts: {}",
                            known.join(", ")
                        );
                        std::process::exit(1);
                    }
                }
            }
        },
        // Read-only lookup over the DSL rule data compiled INTO this binary (`zzop_summary::explain`,
        // never `docs/rules/catalog.md` prose — see that function's doc). Fixed arity (unlike
        // `contract`'s optional name): a rule id is always required. Same two-lane contract as
        // `contract`: a dash-shaped/missing/extra id is a usage error (exit 2, `run_lookup`); a
        // real-but-unexplainable id (unknown, ambiguous, a whole pack, or a native analysis id) is a
        // runtime lookup failure (exit 1, `explain`'s own `Err` message).
        Some("explain") => run_lookup(&args, "explain <rule-id>", zzop_summary::explain),
        // The version surface, in the two forms the one owner (`zzop_facade::version`) publishes: the
        // BARE version (`zzop_summary::version()` = `CARGO_PKG_VERSION`, the workspace release version
        // shared with the `zzop-mcp` binary and MCP `initialize`, so all three can never disagree), and
        // — behind `--verbose` — the DIAGNOSTIC form (`version_string()`: the same version plus every
        // parser's `PARSER_FINGERPRINT`, the cache-key ingredient that says which parser build produced
        // an analysis). The bare form stays the default on purpose: it is a one-token line that scripts
        // and this repo's own tests parse, and lengthening it would break them for a diagnostic almost
        // no invocation wants. `zzop-mcp version --verbose` prints the identical string.
        Some("version") | Some("--version") => match args.get(2).map(String::as_str) {
            None => println!("zzop {}", zzop_summary::version()),
            Some("--verbose") if args.len() == 3 => println!("{}", zzop_summary::version_string()),
            // Names the FIRST argument that is not `--verbose`, never `--verbose` itself: the
            // `args.len() == 3` arm above falls through here on `version --verbose extra`, and
            // reporting `"--verbose"` would point the reader at the one argument that was correct.
            Some(_) => {
                let bad = args[2..].iter().find(|a| *a != "--verbose");
                eprintln!("usage: zzop version [--verbose] (unexpected argument {bad:?})");
                std::process::exit(2);
            }
        },
        Some("help") | Some("--help") | Some("-h") => print_help(),
        _ => {
            eprintln!("{USAGE}");
            eprintln!("{BARE_INVOCATION_HINT}");
            std::process::exit(2);
        }
    }
}

/// Every non-diverging handler's terminal step: `Ok` to stdout, `Err` as `zzop: <message>` to stderr +
/// exit 1. (`cli::print_or_exit` is the diverging-return twin for the `run_*` helpers, which never
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
