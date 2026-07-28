//! SUBCOMMAND RUNNERS — the branches whose argv parsing is big enough to deserve its own function.
//! Every `run_*` here diverges (`-> !`): it parses, calls the shared `zzop_summary` library, prints, and
//! exits, so `main.rs`'s match arm is one call. The exit-code contract is the same everywhere: 2 =
//! argument-shape error, 1 = runtime failure (unreadable file / invalid / refused).

use super::args::{parse_trees_args, reject_flag_like_args};
use super::{print_or_exit, read_or_exit};

/// `graph <path>... [--scope <prefix>] [--top <n>]` / `graph --config <path> [...]`: the two optional
/// honesty knobs are lifted out of argv HERE, and everything left over goes through the shared
/// [`parse_trees_args`] so this subcommand inherits the identical silent-narrowing guards its siblings
/// have (a dropped trailing path after `--config`, a dash-shaped path swallowed as a root). Both knobs
/// take a value; a missing, dash-shaped or non-numeric one is an argument-shape mistake (exit 2), never
/// a silently-ignored option. `--top` has no upper bound on purpose — this is a file/pipe surface like
/// `facts`/`manifest`, not the cap-governed MCP wire — so the only rejections are "not a number" and
/// "negative", both of which `usize` parsing already refuses.
pub fn run_graph(args: &[String]) -> ! {
    const USAGE_GRAPH: &str = "usage: zzop graph <path>... | graph --config <zzop.config.jsonc> [--domain <join|dep|risk|posture>] [--format <mermaid|cosmograph-nodes|cosmograph-links>] [--scope <prefix>] [--top <n>]";
    let mut rest: Vec<String> = args[..2.min(args.len())].to_vec();
    let (mut scope, mut top) = (None, None);
    let mut domain: Option<zzop_summary::GraphDomain> = None;
    let mut format = zzop_summary::GraphFormat::Mermaid;
    let mut i = 2;
    while i < args.len() {
        let flag = args[i].as_str();
        if flag == "--scope" || flag == "--top" || flag == "--domain" || flag == "--format" {
            let Some(value) = args.get(i + 1).filter(|v| !v.starts_with('-')) else {
                eprintln!("{USAGE_GRAPH} ({flag} needs a value)");
                std::process::exit(2);
            };
            if flag == "--scope" {
                scope = Some(value.clone());
            } else if flag == "--format" {
                // Same one-owner rule as `--domain`: the accepted set is read off `WIRE_NAMES`, so a new
                // format cannot ship with a usage line that does not mention it.
                match zzop_summary::GraphFormat::from_wire(value) {
                    Some(f) => format = f,
                    None => {
                        eprintln!(
                            "{USAGE_GRAPH} (--format must be one of {}, got {value:?})",
                            zzop_summary::GraphFormat::WIRE_NAMES.join("|")
                        );
                        std::process::exit(2);
                    }
                }
            } else if flag == "--domain" {
                // An unknown domain is a usage error, never a silently-empty diagram. The accepted set
                // is read off `GraphDomain::WIRE_NAMES`, so a new domain cannot ship with a stale
                // usage line — the same one-owner rule `Language::WIRE_NAMES` follows.
                match zzop_summary::GraphDomain::from_wire(value) {
                    Some(d) => domain = Some(d),
                    None => {
                        eprintln!(
                            "{USAGE_GRAPH} (--domain must be one of {}, got {value:?})",
                            zzop_summary::GraphDomain::WIRE_NAMES.join("|")
                        );
                        std::process::exit(2);
                    }
                }
            } else {
                match value.parse::<usize>() {
                    Ok(n) => top = Some(n),
                    Err(_) => {
                        eprintln!(
                            "{USAGE_GRAPH} (--top needs a non-negative integer, got {value:?})"
                        );
                        std::process::exit(2);
                    }
                }
            }
            i += 2;
            continue;
        }
        rest.push(args[i].clone());
        i += 1;
    }
    let (paths, config_path) = parse_trees_args(&rest, "graph", 1);
    if format.is_cosmograph() {
        // Two refusals rather than two silent accommodations, because a flag that is accepted and then
        // ignored is the failure class this repo's message audit named ("no knob that does nothing").
        // The domain restriction is stated with its REASON so the message teaches the boundary instead of
        // merely enforcing it.
        if domain != Some(zzop_summary::GraphDomain::Dep) {
            eprintln!(
                "{USAGE_GRAPH} (--format cosmograph-* requires --domain dep — the only domain whose \
                 graph outgrows a flowchart; join/risk/posture are tens of nodes, where mermaid is \
                 strictly better)"
            );
            std::process::exit(2);
        }
        if top.is_some() {
            eprintln!(
                "{USAGE_GRAPH} (--top does not apply to --format cosmograph-*: the lane is UNCAPPED \
                 because a viewer with zoom does the job a node cap does for a drawn picture)"
            );
            std::process::exit(2);
        }
        let out = match zzop_summary::graph_cosmograph(
            &paths,
            config_path,
            scope.as_deref(),
            format == zzop_summary::GraphFormat::CosmographLinks,
        ) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        };
        // stdout stays a pure data table for the viewer; the honesty channel rides stderr so it survives
        // `zzop graph ... > links.ndjson` without corrupting a row.
        print!("{}", out.data);
        eprintln!("{}", out.census);
        std::process::exit(0);
    }
    print_or_exit(zzop_summary::graph_mermaid(
        &paths,
        config_path,
        scope.as_deref(),
        top,
        domain.unwrap_or(zzop_summary::GraphDomain::Join),
    ));
}

/// `validate-envelope` / `validate-rule-pack`: read the one path arg, run the offline check, print the
/// report VERBATIM, and exit BY VALIDITY (0 valid, 1 invalid) so scripts/CI can gate on it — the two
/// subcommands' own exit contract. Missing/extra/flag-shaped args exit 2, an unreadable file exits 1. The
/// reports are NOT one shape (only the envelope's has a `hints` pass), so this reads `valid` and no more.
pub fn run_file_validate(args: &[String], usage_tail: &str, validate: fn(&str) -> String) -> ! {
    let usage = format!("usage: zzop {usage_tail}");
    let Some(path) = args.get(2) else {
        eprintln!("{usage}");
        std::process::exit(2);
    };
    if args.len() > 3 {
        eprintln!("{usage} (one file — got {})", args.len() - 2);
        std::process::exit(2);
    }
    reject_flag_like_args([path.as_str()], &usage);
    let text = read_or_exit(path);
    let report = validate(&text);
    println!("{report}");
    // The report is `{"valid":bool,…}` — deserialize just to read `valid` for the exit code; a
    // never-fails report that somehow doesn't parse is treated as invalid (exit 1), never a false pass.
    let valid = serde_json::from_str::<serde_json::Value>(&report)
        .ok()
        .and_then(|v| v.get("valid").and_then(serde_json::Value::as_bool))
        .unwrap_or(false);
    std::process::exit(if valid { 0 } else { 1 });
}

/// `explain <rule-id>`: the one-arg "call a `Result<String, String>` lookup and print" shape shared by
/// any read-only subcommand that answers from in-process data rather than a file — today just
/// `zzop_summary::explain`. Missing/extra/flag-shaped args exit 2 (same usage-error contract as
/// every sibling subcommand); `Ok` prints to stdout and exits 0; `Err` prints `zzop: <message>` to
/// stderr and exits 1 (a runtime lookup failure, never a usage error — the id was well-formed, just not
/// explainable).
pub fn run_lookup(
    args: &[String],
    usage_tail: &str,
    lookup: fn(&str) -> Result<String, String>,
) -> ! {
    let usage = format!("usage: zzop {usage_tail}");
    let Some(query) = args.get(2) else {
        eprintln!("{usage}");
        std::process::exit(2);
    };
    if args.len() > 3 {
        eprintln!("{usage} (one id — got {})", args.len() - 2);
        std::process::exit(2);
    }
    reject_flag_like_args([query.as_str()], &usage);
    print_or_exit(lookup(query));
}

/// `diff <a.json> <b.json> [--allow-tool-drift]`: read two already-produced manifests and print their
/// delta. `--allow-tool-drift` is the ONE recognized flag (it downgrades the cross-build refusal to a
/// disclosed comparison — see `zzop_summary::manifest::diff`); it may sit anywhere among the two file
/// arguments, and any OTHER dash-shaped argument is a usage error rather than a filename. A refused
/// cross-build diff exits 1 (a runtime refusal — both arguments were well-formed), like every other
/// handler `Err`.
pub fn run_diff(args: &[String]) -> ! {
    const USAGE_DIFF: &str = "usage: zzop diff <a.json> <b.json> [--allow-tool-drift]";
    let mut allow_tool_drift = false;
    let mut files: Vec<&str> = Vec::new();
    for arg in &args[2..] {
        match arg.as_str() {
            "--allow-tool-drift" => allow_tool_drift = true,
            other => files.push(other),
        }
    }
    if files.len() != 2 {
        eprintln!("{USAGE_DIFF} (two manifest files — got {})", files.len());
        std::process::exit(2);
    }
    reject_flag_like_args(files.iter().copied(), USAGE_DIFF);
    let (a, b) = (read_or_exit(files[0]), read_or_exit(files[1]));
    print_or_exit(zzop_summary::diff_manifests_json(&a, &b, allow_tool_drift));
}

/// `zzop init [--force]`: writes the embedded `config-template` document — the ONE canon behind all three
/// surfaces (this, `zzop contract config-template`, MCP `resources/read`) — to the config filename in the
/// current directory; argv parsing plus one file write, no template text. An existing file is never
/// overwritten without `--force`: a RUNTIME refusal (exit 1) like `diff`'s, where a bad argument is exit 2.
pub fn run_init(args: &[String]) -> ! {
    if let Some(bad) = args[2..].iter().find(|a| *a != "--force") {
        eprintln!("usage: zzop init [--force] (unexpected argument {bad:?})");
        std::process::exit(2);
    }
    // Every argument that survived the check above IS `--force`, so any argument at all means forced.
    let force = args.len() > 2;
    let target = zzop_summary::contracts::CONFIG_TEMPLATE_FILENAME;
    let doc = zzop_summary::contracts::find(zzop_summary::contracts::CONFIG_TEMPLATE_NAME)
        .expect("the config-template document is embedded in this binary");
    if !force && std::path::Path::new(target).exists() {
        eprintln!("zzop: {target} exists — pass --force to overwrite it");
        std::process::exit(1);
    }
    print_or_exit(
        std::fs::write(target, doc.content)
            .map(|()| format!("wrote {target}"))
            .map_err(|e| format!("failed to write {target}: {e}")),
    );
}
