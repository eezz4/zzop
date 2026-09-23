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
            "analyze-envelope <envelope.json> — Mode A: a Normalized-AST envelope file REPLACES native parsing, print the same JSON findings summary. The envelope FILE's own directory is searched for a zzop.config.jsonc, the same discovery `analyze <path>` makes at a tree root, and that config's `vocabulary` block and rule-pack selection (`packs.extraDirs`, `packs.disabled`, `packs.only`) are applied — the keys an envelope run can use, since every other one configures a tree walk this lane never performs. The pack axis matters most here: every bundled rule targets zzop's own native filetypes, so an envelope for a language zzop has no parser for gets DSL findings only from a pack that targets ITS extension — declare the directory, or drop the pack in `zzop/rules/` beside the config and it is discovered with no declaration. Applying the config is reported in the reply's `configWarnings`; with no such file beside the envelope the run judges by zzop's built-in convention vocabulary, which cannot know what THIS project calls its own guards".to_string(),
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
            "cross <path> <path>... (2+ paths) | cross --config <path> — analyze 2+ trees, print the cross-layer join".to_string(),
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
            "manifest <path> <path>... (2+ paths) | manifest --config <path> — print the run's STRUCTURAL CONTRACT MANIFEST (identity only: provides/edges/bucket membership, no file or line). Commit it, then compare a later run with `zzop diff`. Like `zzop cross`, it is a CROSS-TREE contract: a single-tree config is refused too, so there is no one-tree manifest to name — use `zzop facts`/`zzop coverage` for a single tree".to_string(),
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
            "coverage <path>... | coverage --config <path> — the AGGREGATE-VISIBILITY view: \"how much of this tree does zzop actually see?\" Per tree, an extension-by-dispatch table (structural / lexical-only / degraded, plus inDepGraph — files of the extension with at least one RESOLVED outgoing import edge — and declaredImports, the pre-resolution declared-specifier denominator (null = never measured, e.g. a prisma/sql row), so declared-but-unresolved import blindness reads off one row; each field's meaning shipped in the reply as dispatchMeaning), blindSpots — the CAPABILITY axis: per-rule evidence blind spots derived from the compiled-in sightline declarations crossed with the tree's structural extensions, with blindSpotBasis saying what was crossed AND what it excluded — unreadExtensions, the principal filetypes no structural parser read, which is the population that cross cannot contain by construction (a language with no parser never becomes a structural extension, so the largest blind spot is the one that vanishes from the list) — ioChannels, whose extracted rows carry one entry per read io kind PRESENT EVEN AT ZERO, so a filled channel cannot vouch for an empty one, and whose zeroExtraction rows name each (channel, extension) this build declares a route/table recognizer for and got zero from, restricted to extensions that are a principal share of what this run read structurally so a rounding-error filetype is ABSENT from the list rather than cleared by it, and keyed on the tree's own extension mix rather than on recognizing a framework by name — the tree's own engine warnings forwarded verbatim (the framework-silence self-reports ride there), the coverage census, and joinVisibility — the tree's own io contribution as COUNTS (provides, consumesKeyed, consumesUnresolved) plus a meaning saying what a key does and does not prove; no rate is derived for you, because 1-of-1 and 400-of-440 are not the same evidence and a quotient hides which one you hold. DELIBERATELY NO SINGLE SCORE: axes zzop never measured on your tree (recall) ride in an unmeasured FIELD instead of being folded into a number that would get quoted without them".to_string(),
        ),
        (
            "map",
            "map <path>... | map --config <path> [--fold <n>] — the MODULE MAP as data: \"what IS this codebase?\" One module is the first --fold segments of a file path (default 1, the top-level map), and the reply is {modules, edges, census, meaning}. This is the same fold `graph --domain dep --fold <n>` draws, with the picture taken off: a row here is {id, files, inCycle} plus lines/linesMeasuredOver when this run measured any of the module's files, and an edge is {from, to, fileEdges} where fileEdges counts the FILE-level imports that collapsed into it. READ TWO NUMBERS AS A PAIR: edges holds only edges BETWEEN modules — an import inside one module is what a module IS — while census.fileImports counts every file-level import including those, so the smaller number is not a subset failure. NOTHING IS CAPPED and there is no --top: every module and edge at this depth is present, and the answer to \"too big to read\" is a higher --fold, which makes the boxes bigger rather than hiding some. A TOPOLOGY, never a verdict — no severity, no score, no ranking, and no findings; for those use `analyze`, and for whether this tree's zeros mean anything use `coverage`. Unlike `graph`, this lane HAS an MCP twin (the `module_map` tool), which is why it exists: the folded map was zzop's best short answer to the headline question and was reachable only from a terminal".to_string(),
        ),
        (
            "graph",
            format!(
                "graph <path>... | graph --config <path> [--domain <{}>] [--format <mermaid|cosmograph-nodes|cosmograph-links>] [--scope <prefix>] [--top <n>] [--fold <n>] — print one PICTURE of the run for an external renderer (zzop draws nothing). --domain picks WHICH RELATION connects two nodes, and each draws different NODES: join = the cross-layer join over io keys (default), dep = the file import graph with cycles marked, risk = blast-radius hubs and extraction seams, posture = mutating routes and their guard status, cochange = files that change together in git history (a SAMPLE of history, not imports — its own picture rather than an overlay on dep because the two edge kinds are true in different ways). --fold <n> answers the OTHER question — what a node IS — by drawing each path's first n segments as one box: `--domain dep --fold 2` turns a thousand-file import graph into the module map it is a picture of, and `--fold 1` into the top-level one. It applies to {} only (the domains whose nodes are paths) and is REFUSED elsewhere rather than ignored: risk/posture nodes are engine verdicts and join nodes are io keys, neither of which has a path granularity. A folded edge is labelled with the number of FILE-LEVEL edges it collapsed, and the fold's own loss is reported separately from --top's. --format picks the serialization: mermaid (default) writes a flowchart for ANY domain; cosmograph-nodes/cosmograph-links write two NDJSON tables for an interactive viewer and take --domain dep ONLY, UNCAPPED — they refuse --top and --fold rather than ignore them (a viewer with zoom does both jobs itself), and put their census on stderr so stdout stays a parseable table. The mermaid lane is SCOPED by design: --top caps what is drawn, and its default is PER DOMAIN because their densities differ ({}) — a join has tens of relations where an import graph has thousands; --scope keeps rows whose source id or site path starts with <prefix>, and is applied BEFORE any fold so it keeps meaning \"which files are in this picture\". Every cap/filter is disclosed in the document (a %% census plus a visible note node), nodes AGGREGATE call sites (no file/line — use `facts`), and drift VERDICTS/hostRekeyCounts are not rendered at all",
                zzop_summary::GraphDomain::WIRE_NAMES.join("|"),
                zzop_summary::GraphDomain::fold_capable_names()
                    .iter()
                    .map(|n| format!("--domain {n}"))
                    .collect::<Vec<_>>()
                    .join(" / "),
                zzop_summary::GraphDomain::wire_defaults()
                    .iter()
                    .map(|(n, t)| format!("{n} {t}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        (
            "init",
            "init [<dir>] [--force] — write the embedded starter zzop.config.jsonc (the same document `zzop contract config-template` prints) into <dir> (default: the current directory), and add the anchored `**/.zzop/` line to that directory's .gitignore if it is missing. <dir> must ALREADY EXIST — init writes a config INTO a tree, it never creates one. Run it once per tree: `zzop cross ./fe ./be` needs a config in each, and a single config in the parent declares one tree, which cross refuses. Refuses to overwrite an existing one without --force".to_string(),
        ),
        (
            "contract",
            "contract [<name>] — no args lists the embedded doc resources; `contract <name>` prints one".to_string(),
        ),
        (
            "explain",
            "explain <rule-id> [--config <path>] — print one DSL rule's data (full <pack>/<rule> id or an unambiguous bare id). Reads the packs compiled into this binary; --config widens that to the packs that config's trees load, which is how a rule recovered into zzop/rules/ or named by packs.extraDirs becomes explainable".to_string(),
        ),
        (
            "version",
            "version [--verbose] (also spelled `--version` / `-V`) — print this binary's version (equals the MCP serverInfo.version); --verbose adds every parser's fingerprint, which names the parser FRONTEND that read the files (an ID, not a per-build hash)".to_string(),
        ),
    ]
}

/// The SHARED KNOBS block: the findings-view knobs, the run knob, and BOTH CI gates.
/// One string, referenced from the table rather than repeated into it, so the three lanes cannot drift.
///
/// 🔴 `--baseline` joined this const on 2026-09-14 (ledger V238). It had been a second owner — a
/// paragraph spliced into [`subcommand_help`] alone — so `zzop analyze --help` documented it and
/// `zzop --help` did not, while [`knobs_pointer`] told a `zzop --help` reader on three separate lines
/// that "that block says what each one does". 📏 Measured on the shipped binary: the SHARED KNOBS
/// block contained `fail-on` once and `baseline` ZERO times, under three promises to the contrary.
/// This const's own first line already named the failure ("so the three lanes cannot drift") — the
/// splice simply happened outside it.
const FILTER_KNOBS: &str = "  Findings-view knobs (the argv spelling of the same arguments the MCP tool twin takes): --severity <critical|warning|info> (minimum severity in the LIST; counts always cover everything), --rule <id> (a full <pack>/<rule> id, or a bare rule id when it is unambiguous — the same forms `explain` takes, through the same lookup; an id that can be proven to match no rule this run could report is a usage error, exit 2, never a silent empty result), --limit <n> (list cap; 0 = counts only).
  Run knob: --profile-rules adds a `ruleTimings` report (per-rule wall-clock, cost-descending) to the reply. CLI-only — it has no MCP tool twin, because a timing report is a question about a local run rather than an answer about the code, and no zzop.config.jsonc key, because a config declares what is true about the PROJECT while this asks about ONE INVOCATION. On analyze-envelope the report times the envelope lane's own rule classes (symbol-scan/io-scan DSL rules plus the whole-graph analyses that run in Mode A). Cache hits contribute no timing, so profile against a cold cache; the report says so itself and carries the cache counts that prove it.
  CI gate: --fail-on <critical|warning|info> moves the EXIT CODE with the findings — exit 3 when any finding sits at or above that severity, 0 when none does; the reply still goes to stdout in full and one line naming the counts goes to stderr. 3 rather than 1 on purpose: 1 already means \"zzop could not answer\", and a CI log has to tell a broken config apart from a real critical. It reads the COUNTS, so --severity/--rule/--limit narrow the view you print without narrowing the gate — all three, which is why `--rule <one id> --fail-on critical` still fails on a critical from some OTHER rule. A --rule id proven to match nothing outranks this gate: that refusal exits 2, because a threshold applied to a view that cannot be what it claims proves nothing. CLI-only (an MCP tool call has no exit code to move), and no zzop.config.jsonc key (a config declares what is true about the PROJECT; which findings should break THIS pipeline is the pipeline's decision). Refused on cross rather than ignored — that reply has no per-tree severity census, so gate each tree with analyze --fail-on.
  CI gate, the other one: --baseline <file> is a RATCHET — an absent file records what this tree already has and passes; a present one fails (exit 3) only on rules that find MORE than it records, naming each as `rule: was -> now`. Use it to adopt zzop on a repository that already has findings, instead of switching rules off. Counts are per rule over the whole run, so it survives code moving between lines; it cannot see a finding moving between files under one rule. Refused together with --fail-on: two gates, one exit code.";

/// Which subcommands take [`FILTER_KNOBS`] — exactly the analysis lanes whose MCP twin tool declares
/// `severity`/`rule`/`limit` in its input schema (`analyze_repo`, `analyze_envelope`, `cross_repo`).
/// Kept as a list rather than a per-row flag so the parity statement above is readable in one place.
const FILTERED_SUBCOMMANDS: [&str; 3] = ["analyze", "analyze-envelope", "cross"];

/// The one-line stand-in [`print_help`] prints under each filtered subcommand, in place of the whole
/// [`FILTER_KNOBS`] block that lane now prints ONCE at the end.
///
/// # Why a pointer rather than the block
/// The whole-list lane used to print [`FILTER_KNOBS`] three times, byte for byte: 3 × 2,239 characters
/// of a 17,750-character document, so a quarter of `zzop help` was spent saying the same thing twice
/// more. The single-subcommand lane ([`subcommand_help`]) is untouched and still prints the block in
/// full — one subcommand answering alone has no repetition to remove.
///
/// # Why it still spells every knob out
/// Folding the block away must not cost REACHABILITY: a reader who stops at the `cross` entry has to
/// leave knowing `--fail-on` exists. So this line NAMES all five flags; only their behaviour moved.
/// It deliberately does not claim which of them a given subcommand accepts — `cross --fail-on` is
/// refused rather than ignored, and [`crate::cli::fail_on`] owns that refusal. A second owner here is
/// how a help line starts contradicting the binary it describes.
///
/// Interpolating `sub` is load-bearing rather than decorative: it is what keeps the three pointers from
/// being byte-identical lines, i.e. from reintroducing in miniature the duplication this removes.
fn knobs_pointer(sub: &str) -> String {
    format!(
        "  ^ zzop {sub} also reads the SHARED KNOBS block printed once at the end of this help: \
         --severity, --rule, --limit (findings view), --profile-rules (run), --fail-on and --baseline (CI gates). \
         That block says what each one does, and where one of them is refused rather than ignored."
    )
}

/// The heading that introduces the single [`FILTER_KNOBS`] printing, naming its subjects so the block
/// does not sit at the end of the document with nothing saying who it is about.
///
/// Derived from [`FILTERED_SUBCOMMANDS`] rather than hand-spelled: that list is already the one owner
/// of "who takes these knobs", and a hand-copied sentence here is the drift this file keeps refusing.
fn shared_knobs_heading() -> String {
    format!(
        "  SHARED KNOBS — the block below applies to: {}. It is printed once for all of them; each \
         subcommand's own --help prints it inline instead.",
        FILTERED_SUBCOMMANDS.join(", ")
    )
}

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
    println!("{}", crate::usage());
    for (name, text) in elaborations() {
        println!("  {text}");
        if FILTERED_SUBCOMMANDS.contains(&name) {
            println!("{}", knobs_pointer(name));
        }
    }
    // ONCE, after the list, rather than once per filtered subcommand. See `knobs_pointer`.
    println!("{}", shared_knobs_heading());
    println!("{FILTER_KNOBS}");
    println!(
        "  (every subcommand also takes --help/-h for just its own line; the MCP server is the sibling 'zzop-mcp' binary — it speaks JSON-RPC over stdio, not a 'zzop' subcommand)"
    );
}

/// One subcommand's own help text: its elaboration plus, when it takes them, the findings knobs.
/// `None` for a name this binary does not dispatch, so an unknown subcommand keeps the exit-2 lane.
fn subcommand_help(name: &str) -> Option<String> {
    // `main`'s dispatch accepts `--version` and `-V` as aliases of `version`; the gate has to know that
    // too, or `zzop --version --help` exits 2 while `zzop version --help` exits 0 — the same subcommand
    // answering a help request two different ways depending on which spelling the caller used.
    let name = if name == "--version" || name == "-V" {
        "version"
    } else {
        name
    };
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
