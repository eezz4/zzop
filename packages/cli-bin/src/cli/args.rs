//! ARGV SHAPE — the parsers and rejections shared by more than one subcommand branch. Everything here
//! answers "is this argument list well-formed?" and exits 2 when it is not; nothing here prints a result
//! or calls the shared library.

/// The findings-view knobs every analysis subcommand takes: `--severity <critical|warning|info>`,
/// `--rule <id>`, `--limit <n>`. Lifted out of argv HERE (the same "pull the flags, hand the rest to the
/// positional parser" shape [`super::run::run_graph`] uses for `--scope`/`--top`), then handed to the
/// SHARED `FindingFilters::new` — the wire-neutral constructor. No filtering logic lives on this side of
/// the boundary: these three flags are the exact arguments the `severity`/`rule`/`limit` MCP tool
/// arguments parse into, so a CLI run and a tool call filter identically by construction.
///
/// Returns `(argv with the three flags removed, filters)`. Every mistake is an argument-shape error
/// (exit 2), never a silently-ignored option: a missing or dash-shaped value, an unknown severity, and
/// an out-of-range/non-integer limit all exit 2 with the subcommand's usage line — the validation
/// vocabulary itself comes from the shared constructor, so the two hosts reject the same values.
pub fn extract_finding_filters(
    args: &[String],
    usage: &str,
) -> (Vec<String>, zzop_summary::FindingFilters) {
    let (mut severity, mut rule, mut limit) = (None, None, None);
    let mut rest: Vec<String> = args[..2.min(args.len())].to_vec();
    let mut i = 2;
    while i < args.len() {
        let flag = args[i].as_str();
        if matches!(flag, "--severity" | "--rule" | "--limit") {
            let Some(value) = args.get(i + 1).filter(|v| !v.starts_with('-')) else {
                eprintln!("{usage} ({flag} needs a value)");
                std::process::exit(2);
            };
            match flag {
                "--severity" => severity = Some(value.clone()),
                "--rule" => rule = Some(value.clone()),
                _ => match value.parse::<usize>() {
                    Ok(n) => limit = Some(n),
                    Err(_) => {
                        eprintln!("{usage} (--limit needs a non-negative integer, got {value:?})");
                        std::process::exit(2);
                    }
                },
            }
            i += 2;
            continue;
        }
        rest.push(args[i].clone());
        i += 1;
    }
    match zzop_summary::FindingFilters::new(severity.as_deref(), rule.as_deref(), limit) {
        Ok(filters) => (rest, filters),
        Err(e) => {
            eprintln!("{usage} ({e})");
            std::process::exit(2);
        }
    }
}

/// Lifts the boolean `--profile-rules` out of argv, the same "pull the flag, hand the rest to the
/// positional parser" shape [`extract_finding_filters`] uses. Returns `(argv with the flag removed,
/// knobs)`.
///
/// A VALUELESS flag, unlike the three findings knobs: `--profile-rules` turns instrumentation on for
/// this run and has nothing to parametrize, so `--profile-rules true` would be a path in the next
/// position and is rejected there by the positional parser rather than silently consumed here.
///
/// Deliberately NOT a `zzop.config.jsonc` key (see `zzop_facade::AnalyzeRequest::profile_rules`): a
/// config file declares what is true about the PROJECT and gets committed, while a timing report is a
/// question about THIS invocation on THIS machine — a committed `profileRules: true` would make every
/// CI run pay for and emit a report nobody asked for.
pub fn extract_run_knobs(args: &[String]) -> (Vec<String>, zzop_summary::RunKnobs) {
    let mut knobs = zzop_summary::RunKnobs::default();
    let mut rest: Vec<String> = args[..2.min(args.len())].to_vec();
    for arg in args.iter().skip(2) {
        if arg == "--profile-rules" {
            knobs.profile_rules = true;
            continue;
        }
        rest.push(arg.clone());
    }
    (rest, knobs)
}

/// A dash-leading argument in a path/pattern position is NEVER swallowed as a path or pattern —
/// `zzop analyze --nope` must be a usage error, not "path does not exist: --nope". Anything
/// dash-shaped here exits 2 with the subcommand's usage line.
pub fn reject_flag_like_args<'a>(args: impl IntoIterator<Item = &'a str>, usage: &str) {
    for arg in args {
        if arg.starts_with('-') {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    }
}

/// The two-source argv shape shared by every multi-tree subcommand (`cross`, `manifest`, `facts`):
/// either trailing paths, or `--config <file>` with NOTHING after it. Shared rather than copied because
/// the silent-narrowing traps it closes are the same on all three — a trailing path after `--config`
/// would be DROPPED (the user believes it joined the analysis), and a dash-shaped path would be
/// swallowed as one. Returns `(paths, configPath)` with exactly one populated; every shape mistake
/// exits 2 here.
///
/// `min_paths` is the subcommand's own paths-mode arity floor, not a shared constant: `cross`/`manifest`
/// ask a JOIN question and need 2+, while `facts` (like `endpoint`) is meaningful over ONE tree — the
/// join runs fine over a single source, intra-tree edges included. Only the arity differs; every
/// silent-narrowing guard below is identical for all of them.
pub fn parse_trees_args<'a>(
    args: &'a [String],
    sub: &str,
    min_paths: usize,
) -> (Vec<String>, Option<&'a str>) {
    let paths_form = if min_paths >= 2 {
        "<path> <path>... (2+ paths)"
    } else {
        "<path>..."
    };
    let usage = format!("usage: zzop {sub} {paths_form} | {sub} --config <zzop.config.jsonc>");
    let (paths, config_path) = match args.get(2).map(String::as_str) {
        Some("--config") => match args.get(3) {
            Some(cp) => {
                if args.len() > 4 {
                    eprintln!(
                        "usage: zzop {sub} --config <zzop.config.jsonc> (no extra paths — the config's trees define the join)"
                    );
                    std::process::exit(2);
                }
                (Vec::new(), Some(cp.as_str()))
            }
            None => {
                eprintln!("usage: zzop {sub} --config <zzop.config.jsonc>");
                std::process::exit(2);
            }
        },
        _ => (args[2..].to_vec(), None),
    };
    // Paths mode needs `min_paths` paths — fewer is an arg-shape mistake (usage error, exit 2, same as
    // every other malformed invocation here), not a runtime failure. The handlers keep their own "at
    // least 2 paths" error for the MCP tool path, where exit codes don't exist.
    if config_path.is_none() && paths.len() < min_paths {
        eprintln!("{usage}");
        std::process::exit(2);
    }
    // Only the leading `--config` above is a recognized flag — a dash-shaped path (or a misplaced
    // `--config` inside the path list) is a usage error, never a path.
    reject_flag_like_args(paths.iter().map(String::as_str).chain(config_path), &usage);
    (paths, config_path)
}
