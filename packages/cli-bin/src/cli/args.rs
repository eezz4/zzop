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
                "--rule" => rule = Some(resolve_rule_filter(value, usage)),
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

/// Resolves a `--rule` value to the spelling a finding's `ruleId` actually carries, and refuses the one
/// class of id that can be PROVEN wrong without running anything.
///
/// # Why this is not a second lookup
/// `zzop explain <rule-id>` already accepts both forms a reader has — the full `<pack>/<rule>` id every
/// finding carries, and a bare `<rule>` id when it is unambiguous across the bundled packs — so this
/// calls THAT (`zzop_summary::explain`, the re-exported facade entry point) and reads the canonical id
/// off the first line of its render. A second resolver here would be a second answer to "what is this
/// id", and the two would drift the day a pack ships a colliding bare name.
///
/// # What changes for the caller
/// A bare DSL id used to filter NOTHING: every DSL finding's `ruleId` is `<pack>/<rule>` and the shared
/// filter compares for exact equality, so `--rule weak-crypto` returned `shown: 0` on a tree that had
/// the finding — the same empty result a typo produces. It is expanded to `security/weak-crypto` here.
///
/// # The one refusal, and why only this one
/// A bare id that names neither a bundled DSL rule nor a native analysis can never equal any finding's
/// `ruleId`, and that is provable from argv alone — so it exits 2, the code this file uses for every
/// other argument that cannot mean anything. BOTH id spaces are consulted before that refusal is
/// printed, and the second one twice: exactly (`duplicate-route`), and as the TAIL of a namespaced
/// native id (`god-model` for `schema/god-model`). The tail stays refused — a finding carries the full
/// id, so the filter really would match nothing — but it is refused by NAMING that full id rather than
/// by claiming the caller's string is not a native analysis — a denial that `zzop explain`, recommended
/// one clause later in the same sentence, contradicts. A QUALIFIED id is deliberately NOT refused here: its pack
/// may be one this build does not bundle but a tree loads (`zzop/rules/`, `packs.extraDirs`), which
/// this side cannot see. That case is judged after the run instead, against the reply's own
/// `packsLoaded` — see `super::fail_on::gate_or_exit`.
fn resolve_rule_filter(value: &str, usage: &str) -> String {
    if let Ok(rendered) = zzop_summary::explain(value) {
        // `explain`'s render opens with `id: <pack>/<rule>` (crates/facade `explain::render`). Reading
        // it back is what makes the bare form work without this file owning a pack corpus of its own.
        if let Some(id) = rendered
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("id: "))
            .map(str::trim)
        {
            return id.to_string();
        }
    }
    if value.contains('/') {
        return value.to_string();
    }
    let native_ids = zzop_summary::native_analysis_ids();
    if native_ids.iter().any(|id| id == value) {
        return value.to_string();
    }
    // The bare TAIL of a namespaced native id (`god-model` for `schema/god-model`,
    // `route-near-miss` for `cross-layer/route-near-miss`) — still unusable as a filter, because a
    // finding carries the FULL id and this filter compares exactly, so the exit code below does not
    // move. What moves is the sentence: the refusal used to open with "it is not a native analysis
    // id" and close by recommending `zzop explain <id>`, which answers that exact query by naming
    // `schema/god-model`. One binary, two verdicts on whether the thing the caller typed exists, and
    // the wrong one was the half attached to the exit code. Consulting the second id space is what
    // turns a denial into the fix. Resolved only when EXACTLY one registered id ends in it — the same
    // terms the bare DSL lane above is accepted on, and the same terms `zzop explain` uses.
    let tails: Vec<&String> = native_ids
        .iter()
        .filter(|id| id.rsplit_once('/').is_some_and(|(_, tail)| tail == value))
        .collect();
    match tails.as_slice() {
        [full] => {
            eprintln!(
                "{usage} (--rule {value:?} is the bare form of the native analysis id {full:?}, and a \
                 finding carries the FULL id — so this filter would match nothing. Pass \
                 `--rule {full}`.)"
            );
            std::process::exit(2);
        }
        [] => {}
        several => {
            let mut ids: Vec<&str> = several.iter().map(|id| id.as_str()).collect();
            ids.sort();
            eprintln!(
                "{usage} (--rule {value:?} is the bare form of {} native analysis ids and a finding \
                 carries the FULL one — pass one of: {})",
                ids.len(),
                ids.join(", ")
            );
            std::process::exit(2);
        }
    }
    eprintln!(
        "{usage} (--rule {value:?} names no rule: it is neither a native analysis id nor the bare form \
         of one, and a DSL rule's id is always `<pack>/<rule>`, so no finding could ever match it. \
         `zzop explain <id>` resolves a bare id; the `rule-catalog` contract document lists every id \
         this build ships.)"
    );
    std::process::exit(2);
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

/// How a trailing-tree-paths subcommand's ARITY FLOOR is spelled — the one owner of those words for
/// [`parse_trees_args`]'s own refusal below, and the value the help lanes are CHECKED AGAINST.
///
/// The distinction is load-bearing, and an earlier version of this comment got it wrong by claiming the
/// help sites read this function: they do not. `main::usage` and `super::help::elaborations` are literal
/// strings, and `every_tree_path_subcommand_spells_its_arity_floor_in_the_help_and_usage_lines` is what
/// binds them to this spelling — it parses the `parse_trees_args` call sites for each floor and requires
/// the help text to contain `<subcommand> {paths_form(floor)}`. So changing these words does NOT change
/// the help; it turns that test red until someone changes the help too. That is a weaker guarantee than
/// derivation and is written down as such, because a doc claiming the strong one is how the next author
/// edits this and believes the job is done.
///
/// `<path>...` is the universal CLI grammar for "one or more", so a lane whose floor is 2 must never be
/// offered with it. It was, until 2026-08-20: `zzop help` said `manifest <path>...` while
/// `zzop manifest ./tree` exited 2 demanding two — the binary offering a form it refuses. The floor
/// itself stays at each call site (it is that subcommand's contract, not a shared constant); only its
/// SPELLING lives here.
pub const fn paths_form(min_paths: usize) -> &'static str {
    if min_paths >= 2 {
        "<path> <path>... (2+ paths)"
    } else {
        "<path>..."
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
    let usage = format!(
        "usage: zzop {sub} {} | {sub} --config <zzop.config.jsonc>",
        paths_form(min_paths)
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
