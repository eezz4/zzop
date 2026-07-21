//! CLI argv-dispatch helpers shared by `main.rs`'s subcommand match — kept out of the binary entry so
//! it stays a thin dispatch table. Both exit the process directly (a CLI arg mistake is terminal) and
//! carry the exit-code contract: 2 = argument-shape error, 1 = runtime (unreadable file / invalid).

/// A dash-leading argument in a path/pattern position is NEVER swallowed as a path or pattern —
/// `zzop-mcp analyze --help` must be a usage error, not "path does not exist: --help". Anything
/// dash-shaped here exits 2 with the subcommand's usage line.
pub fn reject_flag_like_args<'a>(args: impl IntoIterator<Item = &'a str>, usage: &str) {
    for arg in args {
        if arg.starts_with('-') {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    }
}

/// `validate-envelope` / `validate-rule-pack`: read the one path arg, run the offline check, print the
/// `{"valid":…,"issues":[…]}` report, and exit BY VALIDITY (0 valid, 1 invalid) so scripts/CI can gate
/// on it — the `validate-envelope`/`validate-rule-pack` subcommands' own exit contract. Missing/extra/
/// flag-shaped args exit 2, an unreadable file exits 1, exactly like every sibling subcommand.
pub fn run_file_validate(args: &[String], usage_tail: &str, validate: fn(&str) -> String) -> ! {
    let usage = format!("usage: zzop-mcp {usage_tail}");
    let Some(path) = args.get(2) else {
        eprintln!("{usage}");
        std::process::exit(2);
    };
    if args.len() > 3 {
        eprintln!("{usage} (one file — got {})", args.len() - 2);
        std::process::exit(2);
    }
    reject_flag_like_args([path.as_str()], &usage);
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("zzop-mcp: failed to read {path}: {e}");
            std::process::exit(1);
        }
    };
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
