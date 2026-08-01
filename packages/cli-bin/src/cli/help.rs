//! HELP OUTPUT — the usage elaboration, one entry per subcommand. Pure printing: nothing here reads a
//! result or runs an analysis. ONE table backs both help lanes ([`print_help`], the whole list, and
//! [`handle_subcommand_help`], a single row), so `zzop help` and `zzop cross --help` can never describe
//! the same subcommand differently.

/// The per-subcommand elaboration table: `(subcommand, one-line elaboration)`, in the order `zzop help`
/// prints them. Built at call time rather than declared `const` because two rows interpolate a shared
/// library value (`DEFAULT_GRAPH_TOP`) — a hand-copied number here is exactly the drift this repo keeps
/// paying for.
///
/// Deliberately NOT a copy of anything the MCP tool descriptions own. Where a fact belongs to the shared
/// analysis rather than to this binary's argv (the endpoint query's sealed verdict vocabulary being the
/// standing example), the line points at where the answer rides — `check_endpoint`'s reply and
/// `zzop endpoint`'s output both carry `verdictMeaning`, so neither surface's prose is a second owner of
/// what a verdict token means.
fn elaborations() -> Vec<(&'static str, String)> {
    vec![
        (
            "analyze",
            "analyze <path> | analyze --config <zzop.config.jsonc> — analyze ONE repo/tree, print a JSON findings summary. `<path>` auto-discovers <path>/zzop.config.jsonc; `--config` names a config at ANY location (the two are mutually exclusive, and a config declaring 2+ trees is refused — that is the cross-layer join's question)".to_string(),
        ),
        (
            "analyze-envelope",
            "analyze-envelope <envelope.json> — Mode A: a Normalized-AST envelope file REPLACES native parsing, print the same JSON findings summary".to_string(),
        ),
        (
            "validate-envelope",
            "validate-envelope <envelope.json> — offline: is this envelope well-formed? print {valid,issues,hints}, exit 0 valid / 1 invalid. The two lists are different axes: `issues` reject the envelope (they alone decide `valid`, and therefore the exit code), while `hints` are accepted shapes that almost certainly are not what you meant — a valid envelope stays valid, exit 0, with a non-empty `hints`. `hints` is always present, empty included".to_string(),
        ),
        (
            "validate-rule-pack",
            "validate-rule-pack <pack.json> — offline: does this DSL rule pack load, and can every rule in it actually fire? print {valid,issues}, exit 0/1".to_string(),
        ),
        (
            "cross",
            "cross <path>... | cross --config <path> — analyze 2+ trees, print the cross-layer join".to_string(),
        ),
        (
            "file",
            "file <path> [--source-id <id>] <tree>... | file <path> [--source-id <id>] --config <path> — definitive \"what does zzop know about THIS FILE?\" query: the tree it belongs to, its verdict, symbols, io facts, both directions of its dependency edges, and every finding anchored in it (uncapped — a single file is bounded, so nothing is dropped and nothing needs disclosing). The verdict answers whether the file was ANALYZED, not whether it is healthy, because an empty findings list means two different things and only the verdict tells them apart; its meaning ships in the reply's verdictMeaning field, so no help text here is a second owner of the vocabulary. --source-id picks WHICH tree answers when several declare the same relative path: without it the first tree by declaration order answers and the reply names the rest in otherTrees, which is a pointer you can then follow".to_string(),
        ),
        (
            "endpoint",
            "endpoint <pattern> <path>... | endpoint <pattern> --config <path> — definitive \"is io key X provided/consumed/joined?\" query. The reply's verdict is one token from a sealed vocabulary and its verdictMeaning field spells out THAT token's meaning in the reply itself, so no help text here (or in any other host) is a second owner of the vocabulary".to_string(),
        ),
        (
            "manifest",
            "manifest <path>... | manifest --config <path> — print the run's STRUCTURAL CONTRACT MANIFEST (identity only: provides/edges/bucket membership, no file or line). Commit it, then compare a later run with `diff`".to_string(),
        ),
        (
            "diff",
            "diff <a.json> <b.json> [--allow-tool-drift] — compare two manifests: bucket TRANSITIONS first (a route leaving `edges` for `unprovidedConsumes` is a broken contract), then per-relation added/removed. Refuses two different zzop builds unless forced".to_string(),
        ),
        (
            "facts",
            "facts <path>... | facts --config <path> — print the run's POST-ASSEMBLY FACTS (per-tree CommonIr + the whole cross-layer join, UNCAPPED) for YOUR OWN rule program to read. zzop emits; it never runs your program and never reads your findings back".to_string(),
        ),
        (
            "coverage",
            "coverage <path>... | coverage --config <path> — the AGGREGATE-VISIBILITY view: \"how much of this tree does zzop actually see?\" Per tree, an extension-by-dispatch table (structural / lexical-only / degraded, plus inDepGraph — files of the extension with at least one RESOLVED outgoing import edge, the per-extension import-resolution sparsity baseline; each field's meaning shipped in the reply as dispatchMeaning), blindSpots — the CAPABILITY axis: per-rule evidence blind spots derived from the compiled-in sightline declarations crossed with the tree's structural extensions, with blindSpotBasis saying what was crossed — the tree's own engine warnings forwarded verbatim (the framework-silence self-reports ride there), the coverage census, and joinVisibility as a sentence. DELIBERATELY NO SINGLE SCORE: axes zzop never measured on your tree (recall) ride in an unmeasured FIELD instead of being folded into a number that would get quoted without them".to_string(),
        ),
        (
            "graph",
            format!(
                "graph <path>... | graph --config <path> [--domain <{}>] [--format <mermaid|cosmograph-nodes|cosmograph-links>] [--scope <prefix>] [--top <n>] — print one PICTURE of the run for an external renderer (zzop draws nothing). --domain picks which picture, and each draws different NODES: join = the cross-layer join over io keys (default), dep = the file import graph with cycles marked, risk = blast-radius hubs and extraction seams, posture = mutating routes and their guard status. --format picks the serialization: mermaid (default) writes a flowchart for ANY domain; cosmograph-nodes/cosmograph-links write two NDJSON tables for an interactive viewer and take --domain dep ONLY, UNCAPPED — they refuse --top rather than ignore it, and put their census on stderr so stdout stays a parseable table. The mermaid lane is SCOPED by design: --top caps what is drawn, and its default is PER DOMAIN because their densities differ ({}) — a join has tens of relations where an import graph has thousands; --scope keeps rows whose source id or site path starts with <prefix>. Every cap/filter is disclosed in the document (a %% census plus a visible note node), nodes AGGREGATE call sites (no file/line — use `facts`), and drift VERDICTS/hostRekeyCounts are not rendered at all",
                zzop_summary::GraphDomain::WIRE_NAMES.join("|"),
                zzop_summary::GraphDomain::wire_defaults()
                    .iter()
                    .map(|(n, t)| format!("{n} {t}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        (
            "init",
            "init [--force] — write the embedded starter zzop.config.jsonc (the same document `zzop contract config-template` prints) into the current directory; refuses to overwrite an existing one without --force".to_string(),
        ),
        (
            "contract",
            "contract [<name>] — no args lists the embedded doc resources; `contract <name>` prints one".to_string(),
        ),
        (
            "explain",
            "explain <rule-id> — print one bundled DSL rule's compiled-in data (full <pack>/<rule> id or an unambiguous bare id)".to_string(),
        ),
        (
            "version",
            "version [--verbose] — print this binary's version (equals the MCP serverInfo.version); --verbose adds every parser's fingerprint, the cache-key ingredient that identifies which parser build produced an analysis".to_string(),
        ),
    ]
}

/// The three findings-view knobs, appended to the elaboration of every subcommand that takes them.
/// One string, referenced from the table rather than repeated into it, so the three lanes cannot drift.
const FILTER_KNOBS: &str = "  Findings-view knobs (the argv spelling of the same arguments the MCP tool twin takes): --severity <critical|warning|info> (minimum severity in the LIST; counts always cover everything), --rule <id>, --limit <n> (list cap; 0 = counts only).";

/// Which subcommands take [`FILTER_KNOBS`] — exactly the analysis lanes whose MCP twin tool declares
/// `severity`/`rule`/`limit` in its input schema (`analyze_repo`, `analyze_envelope`, `cross_repo`).
/// Kept as a list rather than a per-row flag so the parity statement above is readable in one place.
const FILTERED_SUBCOMMANDS: [&str; 3] = ["analyze", "analyze-envelope", "cross"];

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
    for (name, text) in elaborations() {
        println!("  {text}");
        if FILTERED_SUBCOMMANDS.contains(&name) {
            println!("{FILTER_KNOBS}");
        }
    }
    println!(
        "  (every subcommand also takes --help/-h for just its own line; the MCP server is the sibling 'zzop-mcp' binary — it speaks JSON-RPC over stdio, not a 'zzop' subcommand)"
    );
}

/// One subcommand's own help text: its elaboration plus, when it takes them, the findings knobs.
/// `None` for a name this binary does not dispatch, so an unknown subcommand keeps the exit-2 lane.
fn subcommand_help(name: &str) -> Option<String> {
    // `main`'s dispatch accepts `--version` as an alias of `version`; the gate has to know that too, or
    // `zzop --version --help` exits 2 while `zzop version --help` exits 0 — the same subcommand
    // answering a help request two different ways depending on which spelling the caller used.
    let name = if name == "--version" { "version" } else { name };
    let text = elaborations()
        .into_iter()
        .find(|(sub, _)| *sub == name)
        .map(|(_, text)| text)?;
    Some(if FILTERED_SUBCOMMANDS.contains(&name) {
        format!("usage: zzop {text}\n{FILTER_KNOBS}")
    } else {
        format!("usage: zzop {text}")
    })
}

/// Answers a per-subcommand help REQUEST before any branch parses argv: `zzop <sub> -h` / `--help`
/// prints that subcommand's own elaboration to STDOUT and exits 0.
///
/// Before this gate, a help request fell into `reject_flag_like_args` (a dash-shaped argument in a path
/// position) and left with exit 2 on stderr — handing an ERROR to the one caller who explicitly asked for
/// help, and violating the repo's own exit contract, where 2 means "your arguments were malformed".
///
/// One gate rather than a copy inside each branch: the branches are what would drift, and a help request
/// needs no per-branch context — the flag anywhere after the subcommand means the same thing everywhere.
/// An UNKNOWN subcommand returns without printing, so `zzop nope --help` still takes the usage-error lane
/// instead of being silently accepted.
pub fn handle_subcommand_help(args: &[String]) {
    let Some(sub) = args.get(1).map(String::as_str) else {
        return;
    };
    if !args[2..].iter().any(|a| a == "-h" || a == "--help") {
        return;
    }
    let Some(text) = subcommand_help(sub) else {
        return;
    };
    println!("{text}");
    std::process::exit(0);
}

#[cfg(test)]
mod tests;
