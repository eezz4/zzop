//! ANALYSIS LANES — the four subcommands that run an engine analysis over argv-supplied sources
//! (`analyze`, `analyze-envelope`, `cross`, `endpoint`). They live here rather than inline in the
//! dispatch table because each carries a SOURCE-MODE choice (a tree root vs a config file) and/or the
//! findings-view knobs, which is real argv parsing rather than a one-line call.
//!
//! Nothing here filters, shapes or analyses anything: each runner parses argv, builds the shared
//! `FindingFilters` through its wire-neutral constructor, calls the one `zzop_summary` function its MCP
//! twin tool calls, and prints. Same exit contract as every sibling — 2 = argument shape, 1 = runtime.

use super::args::{extract_finding_filters, parse_trees_args, reject_flag_like_args};
use super::{print_or_exit, read_or_exit};

/// `analyze <path> | analyze --config <file>` plus the findings knobs. The `--config` mode is the reason
/// this subcommand is no longer fixed-arity: without it a config that does not sit AT the tree root was
/// unreachable from a terminal (the multi-tree siblings all had `--config`; the single-tree entry point
/// did not), so `analyze --config ./ci/zzop.config.jsonc` was simply impossible. The two modes are
/// mutually exclusive and the shared handler enforces that too — this parser only refuses the shapes
/// that would be SILENTLY narrowed here (a trailing path after `--config` would be dropped).
pub fn run_analyze(args: &[String]) -> ! {
    const USAGE: &str = "usage: zzop analyze <path> | analyze --config <zzop.config.jsonc> [--severity <critical|warning|info>] [--rule <id>] [--limit <n>]";
    let (rest, filters) = extract_finding_filters(args, USAGE);
    let (path, config_path) = match rest.get(2).map(String::as_str) {
        Some("--config") => {
            let Some(cp) = rest.get(3) else {
                eprintln!("{USAGE}");
                std::process::exit(2);
            };
            if rest.len() > 4 {
                eprintln!(
                    "usage: zzop analyze --config <zzop.config.jsonc> (no extra paths — the config names the tree)"
                );
                std::process::exit(2);
            }
            (None, Some(cp.as_str()))
        }
        Some(p) => {
            // Fixed arity in paths mode: a trailing extra arg would otherwise be DROPPED silently — the
            // user believes it was analyzed (same never-silent rule as endpoint/contract's guards).
            if rest.len() > 3 {
                eprintln!("{USAGE} (one path — got {})", rest.len() - 2);
                std::process::exit(2);
            }
            reject_flag_like_args([p], USAGE);
            (Some(p), None)
        }
        None => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    print_or_exit(zzop_summary::analyze_summary(path, config_path, &filters));
}

/// `analyze-envelope <envelope.json>` plus the findings knobs — Mode A: the file's content REPLACES
/// native parsing entirely for this run (contrast `analyze`, which walks a real tree). Same handler as
/// the `analyze_envelope` MCP tool, so this CLI form and a tool call give the identical answer.
pub fn run_analyze_envelope(args: &[String]) -> ! {
    const USAGE: &str = "usage: zzop analyze-envelope <envelope.json> [--severity <critical|warning|info>] [--rule <id>] [--limit <n>]";
    let (rest, filters) = extract_finding_filters(args, USAGE);
    let Some(path) = rest.get(2) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    if rest.len() > 3 {
        eprintln!("{USAGE} (one file — got {})", rest.len() - 2);
        std::process::exit(2);
    }
    reject_flag_like_args([path.as_str()], USAGE);
    let envelope_json = read_or_exit(path);
    print_or_exit(zzop_summary::analyze_envelope_summary(
        &envelope_json,
        &filters,
    ));
}

/// `cross <path>... | cross --config <path>` plus the findings knobs. `--config` = config-first mode (the
/// config's trees define the join); trailing paths = config-free paths mode. Mirrors the `cross_repo`
/// tool's two modes, and inherits every silent-narrowing guard from the shared [`parse_trees_args`].
pub fn run_cross(args: &[String]) -> ! {
    const USAGE: &str = "usage: zzop cross <path> <path>... (2+ paths) | cross --config <zzop.config.jsonc> [--severity <critical|warning|info>] [--rule <id>] [--limit <n>]";
    let (rest, filters) = extract_finding_filters(args, USAGE);
    let (paths, config_path) = parse_trees_args(&rest, "cross", 2);
    print_or_exit(zzop_summary::cross_summary(&paths, config_path, &filters));
}

/// `endpoint <pattern> <path>... | endpoint <pattern> --config <path>`: one path = single-tree mode (the
/// `check_endpoint` tool's `path` argument), 2+ = config-free paths mode (`paths`), `--config` =
/// config-first mode (`configPath`), parsed exactly like `cross --config`. Same handler as the MCP tool,
/// so a CLI query and a tool call give the identical answer. No findings knobs: the query's own caps are
/// fixed and its MCP twin exposes no `severity`/`rule`/`limit` either — the two surfaces match.
pub fn run_endpoint(args: &[String]) -> ! {
    const USAGE: &str =
        "usage: zzop endpoint <pattern> <path>... | endpoint <pattern> --config <zzop.config.jsonc>";
    let Some(pattern) = args.get(2) else {
        eprintln!("{USAGE}");
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
    // Config mode takes no trailing paths (exactly ONE of path/paths/configPath — the shared handler's
    // own argument contract); paths mode needs 1+ paths. Either shape mistake is a usage error (exit 2),
    // same as every other malformed invocation here.
    if config_path.is_some() && !rest.is_empty() {
        eprintln!("usage: zzop endpoint <pattern> --config <zzop.config.jsonc> (no extra paths)");
        std::process::exit(2);
    }
    if config_path.is_none() && rest.is_empty() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    // The pattern and every path must be dash-free — `endpoint -x a b` is a usage error, not a pattern
    // query (only the positional `--config` above is a recognized flag).
    reject_flag_like_args(
        std::iter::once(pattern.as_str())
            .chain(rest.iter().map(String::as_str))
            .chain(config_path),
        USAGE,
    );
    let result = match (config_path, rest.len()) {
        (Some(_), _) => zzop_summary::endpoint_summary(pattern, None, &[], config_path),
        (None, 1) => zzop_summary::endpoint_summary(pattern, Some(&rest[0]), &[], None),
        (None, _) => zzop_summary::endpoint_summary(pattern, None, rest, None),
    };
    print_or_exit(result);
}

/// `file <path-target> <tree>... | file <path-target> --config <path>`: the D16 targeting surface's CLI
/// twin. Argument shape is deliberately IDENTICAL to `endpoint` above — a leading target, then the same
/// three tree-resolution modes — because the two are the same kind of question (name a target, get a
/// complete answer) and a caller who learned one should not have to learn the other.
///
/// Same handler as the `check_file` MCP tool. No findings knobs, for the same reason `endpoint` has none:
/// this surface caps nothing, so there is nothing for a `--limit` to mean.
///
/// # `--source-id`, and the parity hole it closes
/// `file_summary` has always taken a `source_id`, and until 2026-07-28 this branch passed `None` on every
/// path while the `check_file` tool passed the caller's — so the MCP host could disambiguate and the CLI
/// could not. That is a direct breach of the hard constraint that both hosts answer identically, and the
/// reply itself made it visible: asking a 17-tree config about `package.json` answers from one tree and
/// lists the other eight in `otherTrees`, i.e. **the CLI reply named a next step only the other host
/// could take**. Measured on `corpus/oss`, not argued.
///
/// The sibling `endpoint` lane has no such flag because `endpoint_summary` has no such parameter — the
/// asymmetry was this lane's alone, which is why the argument shape stays otherwise identical to it.
pub fn run_file(args: &[String]) -> ! {
    const USAGE: &str = "usage: zzop file <path> [--source-id <id>] <tree>... | file <path> [--source-id <id>] --config <zzop.config.jsonc>";
    let Some(target) = args.get(2) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    // Lifted out of argv BEFORE the positional tree-resolution match below, so it may sit anywhere after
    // the target and the three source modes keep parsing exactly as they did. Same shape `graph` uses for
    // its own knobs, including the rule that a missing or dash-shaped value is an argument-shape error
    // rather than a silently-ignored option.
    let mut argv: Vec<&str> = args[3..].iter().map(String::as_str).collect();
    let mut source_id: Option<String> = None;
    if let Some(i) = argv.iter().position(|a| *a == "--source-id") {
        let Some(value) = argv.get(i + 1).filter(|v| !v.starts_with('-')) else {
            eprintln!("{USAGE} (--source-id needs a value)");
            std::process::exit(2);
        };
        source_id = Some((*value).to_string());
        argv.drain(i..=i + 1);
    }
    let args: Vec<String> = std::iter::once(String::new())
        .chain(std::iter::once(String::new()))
        .chain(std::iter::once(target.clone()))
        .chain(argv.iter().map(|s| (*s).to_string()))
        .collect();
    let args = &args[..];
    let (rest, config_path) = match args.get(3).map(String::as_str) {
        Some("--config") => match args.get(4) {
            Some(cp) => (&args[5..], Some(cp.as_str())),
            None => {
                eprintln!("usage: zzop file <path> --config <zzop.config.jsonc>");
                std::process::exit(2);
            }
        },
        _ => (&args[3..], None),
    };
    if config_path.is_some() && !rest.is_empty() {
        eprintln!("usage: zzop file <path> --config <zzop.config.jsonc> (no extra tree paths)");
        std::process::exit(2);
    }
    if config_path.is_none() && rest.is_empty() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    reject_flag_like_args(
        std::iter::once(target.as_str())
            .chain(rest.iter().map(String::as_str))
            .chain(config_path),
        USAGE,
    );
    let sid = source_id.as_deref();
    let result = match (config_path, rest.len()) {
        (Some(_), _) => zzop_summary::file_summary(target, sid, None, &[], config_path),
        (None, 1) => zzop_summary::file_summary(target, sid, Some(&rest[0]), &[], None),
        (None, _) => zzop_summary::file_summary(target, sid, None, rest, None),
    };
    print_or_exit(result);
}
