//! CLI argv-dispatch helpers shared by the `zzop` binary's subcommand match (`src/main.rs`) — kept out of the binary entry so
//! it stays a thin dispatch table. Every `run_*` here exits the process directly (a CLI arg mistake is
//! terminal) and carries the exit-code contract: 2 = argument-shape error, 1 = runtime (unreadable file
//! / invalid / refused). The usage line and the `help` elaboration live here too, next to the parsers
//! that print them.

/// The polite lane: an explicit help REQUEST prints the usage line + one elaboration per subcommand to
/// stdout, exit 0. The exit-2 stderr lane stays a bare usage line + `BARE_INVOCATION_HINT` — an error
/// is a pointer AT `help`, not a tutorial.
///
/// The `USAGE` const itself deliberately stays in `main.rs`, not here beside its printer: the MCP
/// package's `surface_prose` meta-test reads that literal out of `packages/cli-bin/src/main.rs` by
/// path to pin that every MCP tool's CLI twin is named in it. Only this ELABORATION moved out (it is
/// pure output, and `main.rs` is a dispatch table under a 300-line cap) — moving the const too bought
/// nothing and broke a cross-package pin in a package this change does not own.
pub fn print_help() {
    println!("{}", crate::USAGE);
    println!("  analyze <path> — analyze ONE repo/tree, print a JSON findings summary");
    println!(
        "  analyze-envelope <envelope.json> — Mode A: a Normalized-AST envelope file REPLACES native parsing, print the same JSON findings summary"
    );
    println!(
        "  validate-envelope <envelope.json> — offline: is this envelope well-formed? print {{valid,issues}}, exit 0 valid / 1 invalid"
    );
    println!(
        "  validate-rule-pack <pack.json> — offline: does this DSL rule pack load, and can every rule in it actually fire? print {{valid,issues}}, exit 0/1"
    );
    println!(
        "  cross <path>... | cross --config <path> — analyze 2+ trees, print the cross-layer join"
    );
    println!(
        "  endpoint <pattern> <path>... | endpoint <pattern> --config <path> — definitive \"is io key X provided/consumed/joined?\" query"
    );
    println!(
        "  manifest <path>... | manifest --config <path> — print the run's STRUCTURAL CONTRACT MANIFEST (identity only: provides/edges/bucket membership, no file or line). Commit it, then compare a later run with `diff`"
    );
    println!(
        "  diff <a.json> <b.json> [--allow-tool-drift] — compare two manifests: bucket TRANSITIONS first (a route leaving `edges` for `unprovidedConsumes` is a broken contract), then per-relation added/removed. Refuses two different zzop builds unless forced"
    );
    println!(
        "  contract [<name>] — no args lists the embedded doc resources; `contract <name>` prints one"
    );
    println!(
        "  explain <rule-id> — print one bundled DSL rule's compiled-in data (full <pack>/<rule> id or an unambiguous bare id)"
    );
    println!("  version — print this binary's version (equals the MCP serverInfo.version)");
    println!(
        "  (the MCP server is the sibling 'zzop-mcp' binary — it speaks JSON-RPC over stdio, not a 'zzop' subcommand)"
    );
}

/// A dash-leading argument in a path/pattern position is NEVER swallowed as a path or pattern —
/// `zzop analyze --help` must be a usage error, not "path does not exist: --help". Anything
/// dash-shaped here exits 2 with the subcommand's usage line.
pub fn reject_flag_like_args<'a>(args: impl IntoIterator<Item = &'a str>, usage: &str) {
    for arg in args {
        if arg.starts_with('-') {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    }
}

/// The two-source argv shape shared by every multi-tree subcommand (`cross`, `manifest`): either 2+
/// trailing paths, or `--config <file>` with NOTHING after it. Shared rather than copied because the
/// silent-narrowing traps it closes are the same on both — a trailing path after `--config` would be
/// DROPPED (the user believes it joined the analysis), and a dash-shaped path would be swallowed as one.
/// Returns `(paths, configPath)` with exactly one populated; every shape mistake exits 2 here.
pub fn parse_trees_args<'a>(args: &'a [String], sub: &str) -> (Vec<String>, Option<&'a str>) {
    let usage = format!(
        "usage: zzop {sub} <path> <path>... (2+ paths) | {sub} --config <zzop.config.jsonc>"
    );
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
    // Paths mode needs 2+ paths — fewer is an arg-shape mistake (usage error, exit 2, same as every
    // other malformed invocation here), not a runtime failure. The handlers keep their own "at least 2
    // paths" error for the MCP tool path, where exit codes don't exist.
    if config_path.is_none() && paths.len() < 2 {
        eprintln!("{usage}");
        std::process::exit(2);
    }
    // Only the leading `--config` above is a recognized flag — a dash-shaped path (or a misplaced
    // `--config` inside the path list) is a usage error, never a path.
    reject_flag_like_args(paths.iter().map(String::as_str).chain(config_path), &usage);
    (paths, config_path)
}

/// `validate-envelope` / `validate-rule-pack`: read the one path arg, run the offline check, print the
/// `{"valid":…,"issues":[…]}` report, and exit BY VALIDITY (0 valid, 1 invalid) so scripts/CI can gate
/// on it — the `validate-envelope`/`validate-rule-pack` subcommands' own exit contract. Missing/extra/
/// flag-shaped args exit 2, an unreadable file exits 1, exactly like every sibling subcommand.
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
/// `zzop_host::explain::explain`. Missing/extra/flag-shaped args exit 2 (same usage-error contract as
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
    print_or_exit(zzop_host::manifest::diff(&a, &b, allow_tool_drift));
}

/// Reads a file argument or exits 1 (a runtime failure, never a usage error — the argument was
/// well-formed, the file just isn't readable). Shared by every file-taking subcommand.
pub fn read_or_exit(path: &str) -> String {
    match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("zzop: failed to read {path}: {e}");
            std::process::exit(1);
        }
    }
}

/// The shared terminal step: `Ok` to stdout + exit 0, `Err` as `zzop: <message>` to stderr + exit 1.
fn print_or_exit(result: Result<String, String>) -> ! {
    match result {
        Ok(text) => {
            println!("{text}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("zzop: {e}");
            std::process::exit(1);
        }
    }
}
