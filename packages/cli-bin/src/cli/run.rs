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
    // DERIVED, not spelled: `WIRE_NAMES` says it is the one owner of the accepted set so that a new
    // domain cannot ship with a usage line that omits it — and this line spelled the set by hand until
    // 2026-08-06, which made that promise false here while the `--domain` REJECTION message two screens
    // below already derived correctly. A caller was told a domain does not exist and then told it does.
    let usage_graph = format!(
        "usage: zzop graph <path>... | graph --config <zzop.config.jsonc> [--domain <{}>] [--format <mermaid|cosmograph-nodes|cosmograph-links>] [--scope <prefix>] [--top <n>] [--fold <n>]",
        zzop_summary::GraphDomain::WIRE_NAMES.join("|")
    );
    let usage_graph = usage_graph.as_str();
    let mut rest: Vec<String> = args[..2.min(args.len())].to_vec();
    let (mut scope, mut top, mut fold) = (None, None, None);
    let mut domain: Option<zzop_summary::GraphDomain> = None;
    let mut format = zzop_summary::GraphFormat::Mermaid;
    let mut i = 2;
    while i < args.len() {
        let flag = args[i].as_str();
        if flag == "--scope"
            || flag == "--top"
            || flag == "--domain"
            || flag == "--format"
            || flag == "--fold"
        {
            let Some(value) = args.get(i + 1).filter(|v| !v.starts_with('-')) else {
                eprintln!("{usage_graph} ({flag} needs a value)");
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
                            "{usage_graph} (--format must be one of {}, got {value:?})",
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
                            "{usage_graph} (--domain must be one of {}, got {value:?})",
                            zzop_summary::GraphDomain::WIRE_NAMES.join("|")
                        );
                        std::process::exit(2);
                    }
                }
            } else if flag == "--fold" {
                // A depth of 0 would name ONE box for the whole tree. Far more likely a typo than a
                // request, and a picture with a single node is indistinguishable from a broken run — so
                // it is an argument-shape error, like every other value this parser cannot honour.
                match value.parse::<usize>() {
                    Ok(0) => {
                        eprintln!(
                            "{usage_graph} (--fold needs at least 1 path segment; 0 would draw the \
                             whole tree as one box)"
                        );
                        std::process::exit(2);
                    }
                    Ok(n) => fold = Some(n),
                    Err(_) => {
                        eprintln!(
                            "{usage_graph} (--fold needs a positive integer of path segments, got \
                             {value:?})"
                        );
                        std::process::exit(2);
                    }
                }
            } else {
                match value.parse::<usize>() {
                    Ok(n) => top = Some(n),
                    Err(_) => {
                        eprintln!(
                            "{usage_graph} (--top needs a non-negative integer, got {value:?})"
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
    // A fold only means something where a node IS a path — refused rather than ignored elsewhere, and
    // the accepted set is read off `accepts_fold` so a new relation domain cannot ship unmentioned here.
    if fold.is_some() {
        let effective = domain.unwrap_or(zzop_summary::GraphDomain::Join);
        if !effective.accepts_fold() {
            eprintln!(
                "{usage_graph} (--fold applies only to --domain {} — those are the domains whose nodes \
                 are PATHS, so a coarser path is a coarser node. risk/posture nodes are engine verdicts \
                 and join nodes are io keys; neither has a path granularity to fold, and accepting the \
                 flag to do nothing would be worse than refusing it)",
                zzop_summary::GraphDomain::fold_capable_names().join("|")
            );
            std::process::exit(2);
        }
    }
    if format.is_cosmograph() {
        // Two refusals rather than two silent accommodations, because a flag that is accepted and then
        // ignored is the failure class this repo's message audit named ("no knob that does nothing").
        // The domain restriction is stated with its REASON so the message teaches the boundary instead of
        // merely enforcing it.
        if domain != Some(zzop_summary::GraphDomain::Dep) {
            eprintln!(
                "{usage_graph} (--format cosmograph-* requires --domain dep — the only domain whose \
                 graph outgrows a flowchart; join/risk/posture are tens of nodes, where mermaid is \
                 strictly better)"
            );
            std::process::exit(2);
        }
        if top.is_some() {
            eprintln!(
                "{usage_graph} (--top does not apply to --format cosmograph-*: the lane is UNCAPPED \
                 because a viewer with zoom does the job a node cap does for a drawn picture)"
            );
            std::process::exit(2);
        }
        if fold.is_some() {
            eprintln!(
                "{usage_graph} (--fold does not apply to --format cosmograph-*: folding is the DRAWN \
                 picture's answer to too many nodes, and this lane emits the uncapped table precisely \
                 so a viewer can do that itself — interactively, at any depth, without re-running zzop)"
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
            // The THIRD hand-written copy of the error sink until 2026-08-10: a bare `eprintln!("{e}")`
            // + exit(1) that skipped the `zzop:` prefix and the missing-config init hint every other
            // lane gets. Same one-sink rule `main::print_result` records: every lane's `Err` lands in
            // `cli::print_or_exit`. Only the error leg routes there — the success leg below has its own
            // stdout/stderr split (data table + census) that `print_or_exit`'s `println!` cannot carry.
            Err(e) => print_or_exit(Err(e)),
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
        fold,
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

/// `explain <rule-id> [--config <path>]`: a read-only rule lookup, over the packs compiled into this
/// binary or — with `--config` — over the packs that config's trees actually load (`zzop_summary::
/// explain` / `explain_with_config`; see the facade module's `from_config` doc for why the wider corpus
/// is opt-in rather than the default).
///
/// Missing/extra/flag-shaped args exit 2 (same usage-error contract as every sibling subcommand); `Ok`
/// prints to stdout and exits 0; `Err` prints `zzop: <message>` to stderr and exits 1 (a runtime lookup
/// failure, never a usage error — the id was well-formed, just not explainable, and a config that
/// cannot be read is the config's problem, not the caller's grammar).
///
/// This was a GENERIC `run_lookup(args, usage_tail, fn(&str) -> Result<String, String>)` until the
/// `--config` form landed on 2026-08-12. It had exactly one caller the whole time, and the shape it
/// abstracted — "one positional arg, no flags" — is precisely the shape that stopped being true, so
/// keeping it would have meant a generic parameterized over a grammar no subcommand has.
pub fn run_explain(args: &[String]) -> ! {
    const USAGE: &str = "usage: zzop explain <rule-id> [--config <path>]";
    let mut config: Option<&str> = None;
    let mut ids: Vec<&str> = Vec::new();
    let mut rest = args[2..].iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--config" => {
                let Some(path) = rest.next() else {
                    eprintln!("{USAGE} (--config needs a path)");
                    std::process::exit(2);
                };
                config = Some(path.as_str());
            }
            other => ids.push(other),
        }
    }
    if ids.len() != 1 {
        eprintln!("{USAGE} (one id — got {})", ids.len());
        std::process::exit(2);
    }
    // Checked AFTER `--config <path>` is consumed, so a dash-shaped rule id is still refused while the
    // flag's own value never reaches this gate.
    reject_flag_like_args([ids[0]], USAGE);
    print_or_exit(match config {
        Some(path) => zzop_summary::explain_with_config(path, ids[0]),
        None => zzop_summary::explain(ids[0]),
    });
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

mod init;

pub use init::run_init;
