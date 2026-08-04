//! Contracts 15 and 16: no crate BELOW the two products names one host's vocabulary without the other
//! host's twin — because the other host's user reads the same sentence and cannot act on it. 15 is the
//! MCP direction, 16 the CLI direction; they are one doctrine scanned twice, and split into two tests
//! only so a failure names which audience was ignored.
//!
//! The haystack is DERIVED from `crates/` (see [`shared_src_dirs`]) rather than listed, and the needles
//! are derived from the two shipped surfaces (`packages/mcp/src/tools/definitions.rs`,
//! `packages/cli-bin/src/main.rs`), so both axes of "what is checked" follow the repo instead of the
//! last person to remember this file.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::collect_rs_files;
use crate::surface_parity::{cli_only_lane_sources, load_registry};

/// EVERY crate in the workspace's `crates/` tree — the haystack both contracts stand on, with nothing
/// subtracted at crate granularity.
///
/// # Why the whole tree, and why derived (2026-08-01)
///
/// This was a hand list of three directories (`summary`, `config`, `facade`) introduced under the
/// heading "the crates both products call through". That heading was the bug: EVERY crate under
/// `crates/` is linked into both shipped binaries — `zzop` and `zzop-mcp` each build over
/// `zzop-summary`, which pulls `facade` -> `engine` -> `{metrics, git, cache, core}` — so "shared" was
/// never a property that distinguished three of them from the rest. The list simply recorded which
/// crates someone had looked at.
///
/// `crates/engine` was the measured cost. Its exclusion carried no justification at all (the doc
/// explained `core`'s and, at length, `facade`'s, and said nothing about the largest crate in the
/// tree), and a live violation was sitting in the blind spot: `analyze/diagnostics/capability.rs`'s
/// adapter on-ramp note named `zzop contract adapter-guide`, `zzop contract example-envelope` and
/// `zzop contract envelope-schema` with no MCP twin for any of the three, in a message whose own doc
/// claims every named surface is reachable "in BOTH dialects". A second one sat next to it — the
/// uncompilable-rule warning told an MCP caller to run `zzop validate-rule-pack`, a spelling that host
/// cannot type, when `validate_rule_pack` has shipped as an MCP tool the whole time.
///
/// # Why there is no crate-level exemption list any more (2026-08-01, same day)
///
/// Widening to `crates/` left exactly ONE crate-wide exemption, `core`, on the stated grounds that
/// "contract 8 (`kernel_vocabulary.rs`) already forbids it from naming even a RULE id, which is a
/// tighter bound than this contract's". That reason was FALSE, not merely thin: contract 8 scans for
/// registered NATIVE ANALYSIS IDS, a set that contains no host spelling at all, so it is not a bound on
/// this contract's subject in either direction. And a live violation was sitting behind it —
/// `crates/core/src/dsl/diagnostics.rs` told the reader of a skipped rule to
/// ``run `zzop validate-rule-pack` on this pack``, twice, on a sink that is a PUBLIC embedder channel
/// (`dsl::eval_pack_into`) with no way to know which host renders the line. Both sites now name the
/// `validate_rule_pack` MCP twin, the same pairing `capability.rs` uses.
///
/// A wrong reason is worse than a missing one (`working-agreements` §5.5 covers the missing case): an
/// unexplained exemption still invites the next reader to check, while a plausible-sounding one removes
/// the reason to look. So the mechanism itself is gone rather than left empty — an empty
/// `EXEMPT_CRATES` is a slot, and the next spelling that needs excusing would land in it by default at
/// CRATE granularity, blinding a whole tree to buy one message. [`CLI_NO_TWIN_EXEMPTIONS`] is the shape
/// an exemption is allowed to take here: per (file, token), with a both-directions staleness check, so
/// every OTHER host spelling in the same file stays scanned. Nothing to add here means nothing to add
/// there either — take it up per token.
///
/// The previous doc's lesson about `facade` is kept because it is about THIS list's prose, not about
/// that crate: `facade` was once excluded on the grounds that "its text is addressed to a direct
/// embedder, and the audit found no tool name in it", which was false the day it was written
/// (`analyze.rs`'s non-directory-root error told a CLI user that an envelope JSON is
/// `validate_envelope`'s input). A guard whose stated boundary claims an audit found nothing is
/// indistinguishable from one that never looked. State the boundary MECHANICALLY and let the
/// mechanism be the claim — which is now literally true: the boundary is `read_dir`, with no filter.
fn shared_src_dirs() -> Vec<PathBuf> {
    let crates_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates");
    let mut present: Vec<String> = fs::read_dir(&crates_root)
        .unwrap_or_else(|e| panic!("{} must be readable ({e})", crates_root.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    present.sort();

    let dirs: Vec<PathBuf> = present
        .iter()
        .map(|c| crates_root.join(c).join("src"))
        .collect();

    // Non-empty floor on the DERIVATION itself (the subject axis of the same failure the file-count
    // sanity checks below cover): a `read_dir` that stopped resolving returns an empty vector, and an
    // empty haystack passes both contracts without reading a byte.
    assert!(
        dirs.len() >= 7,
        "derived only {} shared crate dir(s) from {} — this workspace has had eight crates since the \
         host split and every one of them is in the haystack, so a number this small is a broken \
         derivation, never a shrunken workspace: {dirs:#?}",
        dirs.len(),
        crates_root.display()
    );
    for dir in &dirs {
        assert!(
            dir.is_dir(),
            "{} is not a directory — every workspace crate keeps its sources under src/, so this is \
             a derivation reading something that is not a crate",
            dir.display()
        );
    }
    dirs
}

/// `packages/mcp/src/tools/definitions.rs` — the shipped `tools/list` payload, read as SOURCE TEXT by
/// relative path. `zzop-engine` does not depend on that package (and must not: the shared analysis
/// engine cannot depend on a product that sits above it), so the tool names are extracted from its
/// source the same way the sibling `surface_parity.rs` reads the facade's pinned key-set literals and
/// `packages/mcp/tests/surface_prose.rs` reads the CLI's `USAGE` const.
const MCP_TOOL_DEFINITIONS: &str =
    include_str!("../../../../packages/mcp/src/tools/definitions.rs");

/// The SHIPPED MCP tool names, derived from [`MCP_TOOL_DEFINITIONS`].
///
/// # Why derived (2026-07-28)
///
/// This was a hand-written array of six tool names, and it was stale: `check_file` shipped in D16 as
/// the seventh tool and nobody came back here. Measured rather than reasoned — planting
/// `"call the check_file tool now"` in `crates/summary/src/lib.rs` left contract 15 GREEN, so a shared
/// crate could tell a CLI user to call a tool that has no CLI spelling and this contract, whose entire
/// job is that sentence, could not see it. A needle list that names its subjects instead of deriving
/// them measures the day it was written, not the surface.
///
/// The extraction is the `"name": "<tool>"` literal each tool object opens with. A JSON *property*
/// named `name` would be spelled `"name": {` (a schema object) and cannot be confused with one. The
/// count is cross-checked against the file's `"inputSchema"` occurrences — an independent structural
/// feature every tool object carries exactly once — so an extraction that silently sees only some of
/// the tools fails instead of narrowing the needle set.
fn shipped_mcp_tool_names() -> Vec<String> {
    let name_re = regex::Regex::new("\"name\":\\s*\"([a-z0-9_]+)\"").expect("static regex");
    let names: Vec<String> = name_re
        .captures_iter(MCP_TOOL_DEFINITIONS)
        .map(|c| c[1].to_string())
        .collect();
    let tool_objects = MCP_TOOL_DEFINITIONS.matches("\"inputSchema\"").count();
    assert_eq!(
        names.len(),
        tool_objects,
        "extracted {} tool name(s) ({names:?}) from packages/mcp/src/tools/definitions.rs, but that \
         file declares {tool_objects} `\"inputSchema\"` block(s) — every tool object has exactly one \
         of each, so the two disagreeing means this extraction is reading only part of the shipped \
         tool surface and the needle list below is silently narrow",
        names.len(),
    );
    assert!(
        !names.is_empty(),
        "no tool names extracted from packages/mcp/src/tools/definitions.rs — the needle list below \
         would then carry no tool name at all and both contracts would scan for the wire arguments \
         alone"
    );
    names
}

/// Wire argument names that have a DIFFERENT spelling on the CLI (`configPath` is `--config`;
/// `envelopeJson`/`packJson` are file arguments) — the residue that genuinely cannot be derived.
///
/// The tool NAMES are derived ([`shipped_mcp_tool_names`]); these are not, because "which of a tool's
/// arguments is respelled on the CLI" is a pairing judgment, not a fact any single artifact states —
/// the same reason `packages/mcp/tests/surface_prose.rs` keeps its tool -> CLI-phrase table by hand
/// while iterating the shipped tool list. Every argument in a schema is NOT a candidate: `path`,
/// `paths`, `pattern`, `severity`, `rule` and `limit` all have honest CLI counterparts and appear in
/// ordinary prose besides.
const MCP_ONLY_WIRE_ARGUMENTS: [&str; 3] = ["configPath", "envelopeJson", "packJson"];

/// MCP-ONLY vocabulary: every shipped tool name, plus the CLI-respelled wire arguments.
fn mcp_only_vocabulary() -> Vec<String> {
    let mut out = shipped_mcp_tool_names();
    out.extend(MCP_ONLY_WIRE_ARGUMENTS.iter().map(|s| (*s).to_string()));
    out
}

/// Contract 15 — no user-facing message built in a shared crate names MCP-only vocabulary.
///
/// WHY THIS IS A CLASS AND NOT FOUR BUGS: `crates/config/src/lib_tests.rs` already pinned exactly this
/// doctrine (`load_config_file`'s missing-config error must name neither `zzop init` nor `--config`,
/// because the shared library is spoken by both hosts) — and while that ONE point was guarded, five
/// siblings drifted the other way and shipped, every one of them reproducible on the real binaries: a
/// multi-tree config told a CLI user to "use the cross_repo tool with configPath"; a single-tree config
/// told them to "use analyze_repo for it"; EVERY paths-mode run's `configWarnings` said "pass configPath
/// to honor it"; a capped edge list said "drill into a specific route with check_endpoint"; and a blank
/// envelope file reported "envelopeJson is empty" to a caller who had passed a FILE. A guard on one
/// point does not defend a class, which is what this test is.
///
/// PRAGMATIC PROXY, same spirit as contract 8's kernel scan. The haystack is every non-test `.rs` file
/// under the shared crates; the needles are looked for only inside STRING LITERALS THAT LOOK LIKE PROSE
/// — a double-quoted literal containing a space, on a line that is not a `//` comment. That boundary is
/// deliberate and load-bearing in both directions:
/// - Doc comments are EXEMPT: they are for the maintainer reading the source, not the user reading a
///   reply, and naming the real tool a function backs is how the code stays navigable.
/// - Space-free literals are EXEMPT: they are identifiers/keys (`args.get("configPath")`, a JSON field
///   name), not sentences. The `operation` parameter that used to carry `"cross_repo"` into a shared
///   error was space-free too, which is precisely why that leak needed a HUMAN fix (it now passes "the
///   cross-layer join") rather than being caught here.
///
/// **What this proves**: no prose-shaped string literal in a shared crate's non-test source contains an
/// MCP-only token.
/// **What this CANNOT prove**: that a message assembled from pieces (a `format!` argument threaded in
/// from elsewhere, a `concat!`) stays clean; or that a spelling-free sentence is actually GOOD advice. A
/// human reading the diff is still the backstop for both. The opposite direction — the CLI's own
/// vocabulary leaking into an MCP client's reading — is
/// [`shared_crate_user_facing_messages_carry_no_cli_only_vocabulary`], contract 16.
#[test]
fn shared_crate_user_facing_messages_carry_no_mcp_only_vocabulary() {
    let (scanned, prose) = shared_prose(&BTreeSet::new());
    assert!(scanned > 20, "sanity: scanned only {scanned} shared files");
    let needles = mcp_only_vocabulary();
    assert_needles_non_empty(&needles, "MCP-only");
    let subcommands = cli_subcommands();
    let hits = offenders_naming(&prose, &needles, |token, norm| {
        mcp_token_has_named_twin(token, norm, &subcommands)
    });
    let offenders = render_offenders(&hits, "MCP-only");
    assert!(
        offenders.is_empty(),
        "a shared crate's user-facing message names MCP-only vocabulary — the SAME sentence reaches a \
         `zzop` CLI user, who cannot call a tool or pass a JSON argument. Make it spelling-free, or take \
         the word from a caller-supplied parameter (see zzop_config::trees's `operation`): {offenders:#?}"
    );
}

/// `crates/facade/src/lib.rs` — the third needle SOURCE (2026-08-03). The first two sources are the
/// two shipped host surfaces; this one is the layer BENEATH them, and it earned its place through a
/// live leak neither host list could see: a ⓖ-era draft of `crates/engine/src/envelope/overlay.rs`'s
/// ignored-`calls` warning told the user the channel lights up "in `analyzeEnvelope`" — the facade
/// entry point's embedder spelling, which is no MCP tool (`analyze_envelope` is) and no CLI
/// subcommand (`zzop analyze-envelope` is), so both existing needle sets scanned right past it while
/// NEITHER audience could act on the word.
const FACADE_LIB: &str = include_str!("../../../facade/src/lib.rs");

/// FACADE-ENTRY-POINT vocabulary — every function name `crates/facade/src/lib.rs` `pub use`s, plus
/// each one's embedder (camelCase, `_json`-stripped) spelling. A user-facing sentence has exactly two
/// legitimate dialects (the MCP tool, the CLI subcommand); the facade's own symbol names are a THIRD
/// spelling only an embedder reading Rust ever sees, so naming one in prose is a leak to BOTH
/// audiences at once — which is why [`shared_crate_user_facing_messages_carry_no_facade_entry_point_names`]
/// grants it no twin excuse at all.
///
/// Derivation and its two filters, each load-bearing:
/// - Only the item names of `pub use` lines (the segment after the last `::`, brace lists split) —
///   the facade's PUBLIC surface, not its module tree. Type names (`AnalyzeRequest`) are skipped by
///   the lowercase-start filter: the incident class is a FUNCTION name in running prose.
/// - Only multi-word names (containing `_`, or a camel hump after derivation): `explain` and
///   `version` are ordinary English that appears in honest sentences, and a needle that matches
///   prose-the-word rather than name-the-symbol would drown this contract in false reds.
/// - Any derived spelling that collides with a real host spelling (a shipped MCP tool name, a
///   dispatched CLI subcommand) is dropped: it is then the OTHER contracts' subject, with their twin
///   rules, not this one's.
fn facade_entry_point_vocabulary() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for item in FACADE_LIB.split("pub use ").skip(1) {
        let Some(decl) = item.split(';').next() else {
            continue;
        };
        let tail = decl.rsplit("::").next().unwrap_or(decl);
        for raw in tail.trim_matches(['{', '}', ' ', '\n', '\r']).split(',') {
            let name = raw.trim().trim_matches(['{', '}']).trim();
            if !name.is_empty() && name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
                names.push(name.to_string());
            }
        }
    }
    assert!(
        names.len() >= 8,
        "extracted only {} lowercase pub-use name(s) from crates/facade/src/lib.rs — the pub-use \
         parse has stopped matching, and a needle list that shrinks silently is what every derived \
         list in this file replaced: {names:?}",
        names.len()
    );

    let tools = shipped_mcp_tool_names();
    let subs = cli_subcommands();
    let is_host_spelling = |n: &str| {
        tools.iter().any(|t| t == n) || subs.iter().any(|s| s == n || s.replace('-', "_") == n)
    };

    let mut out = Vec::new();
    for name in &names {
        if name.contains('_') && !is_host_spelling(name) {
            out.push(name.clone());
        }
        // The embedder spelling the incident wore: `_json` stripped, camelCased. Single-word results
        // (`analyze`) are dropped by the same English-collision filter as above.
        let stem = name.strip_suffix("_json").unwrap_or(name);
        if stem.contains('_') {
            let mut camel = String::new();
            let mut up = false;
            for c in stem.chars() {
                if c == '_' {
                    up = true;
                } else if up {
                    camel.extend(c.to_uppercase());
                    up = false;
                } else {
                    camel.push(c);
                }
            }
            if !is_host_spelling(&camel) {
                out.push(camel);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Contract 15b — the third source's sweep: no user-facing message built in a shared crate names a
/// facade entry point in EITHER spelling (the Rust name or its embedder camelCase). Unlike the two
/// host directions there is NO twin clause: a facade symbol is not a spelling either audience can
/// type, so the fix is always to name the host twins instead (`zzop analyze-envelope` / MCP tool
/// `analyze_envelope`) or to go spelling-free — never to pair the internal name with anything.
#[test]
fn shared_crate_user_facing_messages_carry_no_facade_entry_point_names() {
    let (scanned, prose) = shared_prose(&BTreeSet::new());
    assert!(scanned > 20, "sanity: scanned only {scanned} shared files");
    let needles = facade_entry_point_vocabulary();
    assert_needles_non_empty(&needles, "facade-entry-point");
    let hits = offenders_naming(&prose, &needles, |_, _| false);
    // The ONE seam where an entry-point name IS the audience's spelling: the facade's own
    // `zzop-facade:`-prefixed argument-validation errors, inside `crates/facade` itself. Those
    // sentences answer the DIRECT embedder who literally typed `analyzeEnvelope(badJson)` — and when
    // one surfaces through a shipped host it reports a host bug (the host authors those call
    // arguments itself, so invalid input there is zzop's own defect and the internal seam name is
    // the bug-report anchor). BOTH conditions are required, which is what keeps this narrow: an
    // engine warning naming `analyzeEnvelope` (the ⓖ incident) has neither the prefix nor the path
    // and stays red, and a facade sentence WITHOUT the self-identifying prefix is user-facing prose
    // like any other — two such leaks (`query_coverage.rs`/`query_file.rs`'s "runs over an
    // analyzeTrees output") were found and reworded the day this source landed.
    let hits: Vec<Hit> = hits
        .into_iter()
        .filter(|h| !(h.rel.starts_with("crates/facade/") && h.literal.starts_with("zzop-facade:")))
        .collect();
    let offenders = render_offenders(&hits, "facade-entry-point");
    assert!(
        offenders.is_empty(),
        "a shared crate's user-facing message names a facade entry point — a spelling NO host's user \
         can type (it is the embedder's Rust/JS surface, not the MCP wire and not argv). Name the \
         host twins instead (`zzop <sub>` / the MCP tool), or make the sentence spelling-free: \
         {offenders:#?}"
    );
}

/// CLI-ONLY vocabulary: subcommand spellings and dash-flags, neither of which exists on the MCP wire (a
/// tool call passes a JSON object; there is no argv to put a flag in). Subcommands are spelled `zzop <sub>`
/// rather than bare, because the bare words are ordinary English in this domain — `analyze`, `cross` and
/// `diff` all appear in honest prose that means the OPERATION, not the command.
///
/// Not exhaustive by construction, and deliberately so: `packages/mcp/tests/surface_prose.rs` pins the
/// CLI's `USAGE` literal from the other side, so a subcommand renamed out from under this list is
/// caught there, not here.
///
/// DERIVED, from the two artifacts that already own the two halves.
///
/// This was a 12-entry hand array until 2026-07-28, defended as a "per-lane judgment" rather than a
/// closed set. It was neither — it was just incomplete, and measurably so: `zzop facts`, `graph`,
/// `file`, `manifest`, `diff` and the flags `--top`/`--domain`/`--rule`/`--source-id` were all missing,
/// so planting ``run `zzop facts` to see it`` in `crates/config/src/lib.rs` left this contract green.
/// The same defect class the sweep this repair belongs to is named after.
///
/// Both halves have an owner already:
/// - **subcommands** — every name `packages/cli-bin/src/main.rs` dispatches. CLI-only BY CONSTRUCTION:
///   the `zzop-mcp` binary has no subcommands at all, so any `zzop <sub>` is a spelling only one
///   audience can type. Read as source text, the route this file already uses for the tool names.
/// - **flags** — `crates/config/config-surface.json`'s `cliFlags`, the config vocabulary's own SSOT.
///   A dash-flag is a CLI spelling even when the same knob exists on the wire under another name
///   (`--severity`/`--limit`), which is exactly why the whole list belongs here.
fn cli_only_vocabulary() -> Vec<String> {
    let mut out: Vec<String> = cli_subcommands()
        .iter()
        // BACKTICKED, because that is how this repo writes a command and prose is how it
        // writes a word. Deriving bare `zzop <sub>` matched "a key from a different zzop version"
        // — real prose in an unknown-key warning — on the first run. The old hand list omitted
        // `version`/`file`/`diff`/`graph` for exactly this reason, which read as incompleteness
        // and was partly deliberate; the backtick keeps the completeness without the collision.
        .map(|name| format!("`zzop {name}"))
        .collect();

    const SURFACE: &str = include_str!("../../../config/config-surface.json");
    let surface: serde_json::Value =
        serde_json::from_str(SURFACE).expect("config-surface.json is valid JSON");
    let flags = surface["cliFlags"]
        .as_array()
        .expect("config-surface.json declares a cliFlags array");
    assert!(
        flags.len() > 5,
        "config-surface.json listed {} cli flags — too few to be the real list",
        flags.len()
    );
    // `-h` is a one-character token that would match inside ordinary prose; every other entry is a
    // `--` long form, which cannot.
    out.extend(
        flags
            .iter()
            .filter_map(|f| f.as_str())
            .filter(|f| f.starts_with("--"))
            .map(str::to_string),
    );
    out
}

/// Every subcommand name `packages/cli-bin/src/main.rs` dispatches, bare (no `zzop ` prefix, no
/// backtick) — the half of [`cli_only_vocabulary`] that has a chance of an MCP twin, and the input the
/// twin resolution in [`mcp_token_has_named_twin`] checks a claimed CLI spelling against.
fn cli_subcommands() -> Vec<String> {
    const MAIN: &str = include_str!("../../../../packages/cli-bin/src/main.rs");
    let mut out: Vec<String> = Vec::new();
    let mut rest = MAIN;
    while let Some(i) = rest.find("Some(\"") {
        rest = &rest[i + 6..];
        let Some(end) = rest.find('"') else { break };
        let name = &rest[..end];
        if !name.starts_with('-') && name != "help" {
            out.push(name.to_string());
        }
    }
    assert!(
        out.len() > 5,
        "extracted {} subcommand(s) from main.rs — the dispatch parse has stopped matching, and a \
         needle list that shrinks silently is what this derivation replaced",
        out.len()
    );
    out
}

/// Contract 16 — the mirror of 15: no user-facing message built in a shared crate names CLI-only
/// vocabulary either. Found while writing 15, and it had already shipped one instance: the
/// `config-template` contract resource's description said "Usage: `zzop init [--force]` writes these
/// exact bytes", and `resources/list` serves that same description to an MCP client that can run no
/// subcommand. `crates/config/src/lib_tests.rs` had pinned THIS direction at one point too (that
/// `load_config_file`'s missing-config error names neither `zzop init` nor `--config`) — so both
/// directions were one-point-pinned, and both drifted at a sibling. That symmetry is why 15 alone would
/// have been half a guard.
///
/// SCOPE, and the one exemption that makes it honest: files `docs/contracts/surface-parity.json`
/// declares as `_cliOnlyLanes[<lane>].sources` are subtracted. Those lanes (`zzop manifest`, `diff`,
/// `facts`, `graph`) have NO MCP twin by decision, so their output has exactly one audience and naming
/// the CLI in it is correct, not a leak — `graph`'s mermaid header (`%% zzop graph …`) and its
/// `--top`/`--scope` truncation hint are the live examples. Reusing that registry rather than listing
/// files here means a lane that gains an MCP twin loses the exemption in the same edit that undeclares
/// it, instead of keeping a stale pass.
///
/// **What this CANNOT prove**: the same limits contract 15 states. Note that the STARTER CONFIG
/// (`crates/config/src/template.rs`) is inside the haystack, not exempt — it caught two lines there on
/// its first run ("to see the vocabulary, run: `zzop contract config-surface`" and "`zzop explain
/// <rule-id>` describes one"), and both were reworded to name the config-surface / rule-catalog
/// documents INSTEAD of one route to them. Exempting the template was the other option and was rejected:
/// naming the artifact is strictly better information than naming a command, because it is the same
/// answer for a reader on either surface.
#[test]
fn shared_crate_user_facing_messages_carry_no_cli_only_vocabulary() {
    let excluded = cli_only_lane_sources(&load_registry());
    assert!(
        !excluded.is_empty(),
        "sanity: the CLI-only lane exemption resolved to zero files, which would make this contract \
         scan the CLI-only lanes it is supposed to exempt — surface-parity.json's `_cliOnlyLanes` \
         should have named several"
    );
    let (scanned, prose) = shared_prose(&excluded);
    assert!(scanned > 20, "sanity: scanned only {scanned} shared files");
    let needles: Vec<String> = cli_only_vocabulary();
    assert_needles_non_empty(&needles, "CLI-only");
    let tools = shipped_mcp_tool_names();
    let hits = offenders_naming(&prose, &needles, |token, norm| {
        cli_token_has_named_twin(token, norm, &tools)
    });
    let offenders = render_offenders(&subtract_cli_no_twin_exemptions(hits), "CLI-only");
    assert!(
        offenders.is_empty(),
        "a shared crate's user-facing message names CLI-only vocabulary — the SAME sentence reaches an \
         MCP client, which has no argv and can run no subcommand. Say what to DO, not what to type (see \
         the `config-template` resource description). If the message belongs to a lane that has no MCP \
         twin, declare that lane's sources in docs/contracts/surface-parity.json's `_cliOnlyLanes` \
         instead of rewording: {offenders:#?}"
    );
}

/// Every prose literal in the shared crates' non-test source, paired with its file and starting line —
/// the one haystack both contracts stand on, built once so the two directions can never disagree about
/// what "shared source" means. `excluded` is subtracted by canonical path.
///
/// Returns the file count alongside, so each caller can assert the scan is non-empty: a haystack builder
/// that silently resolves to nothing is how a pin stays green while guarding nothing, which this repo has
/// already been bitten by (`crates/engine/tests/rule_contracts/surface_parity.rs`'s own non-emptiness leg).
/// Byte offset of the first `#[cfg(test)]` that opens a MODULE BODY, or `None` when a file has only
/// declaration-form ones (`#[cfg(test)] mod tests;`) or none at all.
///
/// "Body" is decided by what comes first after the attribute: a `{` (an inline module, whose contents
/// are test source) or a `;` (a declaration, which hides nothing). Deliberately a lexical peek and not
/// a parser — the question is only which of two shapes follows, and both are one token away.
fn find_test_module_body(text: &str) -> Option<usize> {
    let mut from = 0usize;
    while let Some(rel) = text[from..].find("#[cfg(test)]") {
        let at = from + rel;
        let after = &text[at..];
        let brace = after.find('{');
        let semi = after.find(';');
        match (brace, semi) {
            (Some(b), Some(s)) if b < s => return Some(at),
            (Some(_), None) => return Some(at),
            // A declaration, or an attribute with neither — keep looking past it.
            _ => from = at + "#[cfg(test)]".len(),
        }
    }
    None
}

fn shared_prose(excluded: &BTreeSet<PathBuf>) -> (usize, Vec<(PathBuf, usize, String)>) {
    let mut prose = Vec::new();
    let mut scanned = 0usize;
    for dir in shared_src_dirs() {
        let mut files = Vec::new();
        collect_rs_files(&dir, &mut files);
        files.sort();
        // PER-DIRECTORY floor, not just a total. A total-only floor is satisfied by `crates/summary`
        // alone, so a walk that stopped resolving one crate — the case the derivation above exists to
        // prevent from ever being invisible — would still clear it while that crate went unread.
        assert!(
            !files.is_empty(),
            "{} contributed zero .rs files to the shared haystack — every workspace crate has \
             sources, so this is a walk that stopped resolving, not an empty crate",
            dir.display()
        );
        let before = scanned;
        for path in files {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if name.starts_with("test") || name.ends_with("_tests.rs") || name.ends_with("_test.rs")
            {
                continue;
            }
            if fs::canonicalize(&path).is_ok_and(|c| excluded.contains(&c)) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            scanned += 1;
            // Inline `#[cfg(test)] mod tests { … }` blocks are cut off here, for the same reason a
            // `_tests.rs` file is skipped above: a test legitimately asserts on the MCP wire vocabulary
            // (that IS what `zzop_summary::args` validates).
            //
            // Only the BODY form truncates. The old rule cut at the first `#[cfg(test)]` of any shape,
            // justified by "nothing that ships is declared after a file's test module" — which is false
            // for the ordinary `#[cfg(test)] mod tests;` DECLARATION that sits among a file's other
            // `mod` lines, with the whole file below it. Measured 2026-07-28: that made 25 shared-crate
            // files mostly invisible to contracts 15/16 — `crates/summary/src/output/mod.rs` 91%,
            // `crates/config/src/workspaces.rs` 85%, `crates/config/src/lib.rs` 82%. A declaration hides
            // nothing (its tests live in a sibling file this walk already skips by name), so it must not
            // truncate.
            //
            // Still crude, still one-directional: the body form is conventionally last in a file, so
            // cutting there can only narrow the scan, never admit a test literal as shipped prose.
            let source = match find_test_module_body(&text) {
                Some(at) => &text[..at],
                None => &text[..],
            };
            for (lineno, literal) in prose_literals(source) {
                prose.push((path.clone(), lineno, literal));
            }
        }
        // The name/exclusion filters above run per file, so a crate can legitimately lose files here —
        // but never all of them: every crate in the derived set ships non-test source.
        assert!(
            scanned > before,
            "{} contributed .rs files but none survived the test-file and lane-exclusion filters — a \
             crate whose entire src/ reads as test source is a filter bug, not a crate",
            dir.display()
        );
    }
    (scanned, prose)
}

/// The EMPTY-NEEDLE guard both contracts stand on.
///
/// Without it, an empty needle list makes these tests walk the entire shared-crate haystack, find
/// nothing to look for, and report a green — the "a guard over an empty subject set always passes"
/// failure mode this repo has now been bitten by three times. BOTH lists are derived now (contract 16's
/// was hand-written until 2026-07-28), so on either side a broken extraction is what empties them —
/// silently, and with no other symptom. Neither can now go quiet.
fn assert_needles_non_empty(needles: &[String], audience: &str) {
    assert!(
        !needles.is_empty(),
        "the {audience} needle list is EMPTY — this contract would then scan every shared-crate prose \
         literal looking for nothing and pass vacuously"
    );
}

/// One prose literal naming one host-only token — the unit both contracts subtract exemptions from and
/// render offender lines out of.
struct Hit {
    /// Workspace-relative, forward-slashed (`crates/engine/src/...`) so an offender line and an
    /// exemption entry are spelled the same way and can be compared as text.
    rel: String,
    line: usize,
    token: String,
    literal: String,
}

/// A source path from the haystack walk, reduced to its workspace-relative forward-slashed form. The
/// walk builds `<manifest>/../../crates/<crate>/src/...`, so the LAST `/crates/` is the workspace one.
fn rel_path(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    match s.rfind("/crates/") {
        Some(at) => s[at + 1..].to_string(),
        None => s,
    }
}

/// The literal with every run of whitespace collapsed to one space.
///
/// Load-bearing for the twin clause, not cosmetic: this repo's messages are `\`-continued, and the
/// extractor keeps the physical line break plus the next line's indentation inside the literal. A note
/// that reads ``see `zzop contract adapter-guide``` in the source arrives here as
/// ``see `zzop contract \r\n         adapter-guide``` — so a twin check that looked for a contiguous
/// document name would find none and excuse nothing, which is a false RED, and a check that looked for
/// the name anywhere in the literal would excuse a document that has no twin at all, which is a false
/// GREEN. Normalizing first makes both questions answerable on the same string.
fn collapse_whitespace(literal: &str) -> String {
    let mut out = String::with_capacity(literal.len());
    let mut pending_space = false;
    for c in literal.chars() {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(c);
    }
    out
}

/// Every embedded-contract document a literal names in the CLI dialect — the `<doc>` of each
/// ``` `zzop contract <doc>` ```. The subcommand-level needle (``` `zzop contract ```) cannot answer the
/// twin question on its own, because one literal legitimately names several documents and they are
/// twinned INDEPENDENTLY: `capability.rs`'s on-ramp note named four and had a `zzop://contract/` twin
/// for exactly one of them.
fn contract_documents_named(norm: &str) -> Vec<String> {
    const OPEN: &str = "`zzop contract ";
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = norm[from..].find(OPEN) {
        let at = from + rel + OPEN.len();
        let doc: String = norm[at..]
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
            .collect();
        if !doc.is_empty() {
            out.push(doc);
        }
        from = at;
    }
    out
}

/// Does this literal name the MCP twin of the CLI-only `token` it also names?
///
/// # The TWIN clause (2026-08-01), and why it is not a hole
///
/// Contracts 15/16 were written over three crates whose doctrine is SPELLING-FREE: say what to do, not
/// what to type, because the artifact name is the same answer on either surface. Deriving the haystack
/// out of `crates/` brought in `crates/engine`, whose disclosure and framework-silence messages follow a
/// different and equally deliberate rule — name the surface in BOTH dialects, so each audience gets a
/// spelling it can use (``contract: MCP resource `zzop://contract/envelope-guide` on MCP hosts (`zzop
/// contract envelope-guide` with the CLI binary)``). A strict no-spelling scan calls all six of those
/// messages leaks; the doctrine both regimes actually share is the one this clause implements — a shared
/// crate must not name ONE host's spelling WITHOUT its twin.
///
/// The excuse is deliberately narrow, and each narrowing is what keeps it from becoming a rubber stamp:
/// - It is per-TOKEN, never per-literal. The `capability.rs` note that motivated this carried a real
///   `zzop://contract/envelope-guide` and a real `validate_envelope` — a literal-level "mentions the
///   other dialect somewhere" test would have excused the three untwinned documents sitting beside them,
///   i.e. it would have swallowed the exact violation this sweep went looking for.
/// - The twin must EXIST, derived from the shipped surfaces rather than asserted: an MCP tool name comes
///   from `packages/mcp/src/tools/definitions.rs`, a CLI subcommand from `packages/cli-bin/src/main.rs`.
///   Writing ``a `zzop cross-repo` twin`` excuses nothing, because no such subcommand is dispatched.
/// - A dash-flag can never be twinned. There is no argv on the wire, so no MCP spelling of `--severity`
///   exists to name.
///
/// Re-measured against the two catches contracts 15/16 were built on: "use the `cross_repo` tool with
/// `configPath`" names no CLI twin, and ``Usage: `zzop init [--force]``` names no MCP tool `init`.
/// Both stay RED under this clause.
fn cli_token_has_named_twin(token: &str, norm: &str, tools: &[String]) -> bool {
    let Some(sub) = token.strip_prefix("`zzop ") else {
        return false; // a dash-flag
    };
    if sub == "contract" {
        let docs = contract_documents_named(norm);
        return !docs.is_empty()
            && docs
                .iter()
                .all(|doc| norm.contains(&format!("zzop://contract/{doc}")));
    }
    let tool = sub.replace('-', "_");
    tools.iter().any(|t| t == &tool) && names_token(norm, &tool)
}

/// The mirror: does this literal name the CLI twin of the MCP-only `token` it also names? A wire
/// ARGUMENT has no CLI twin by construction (that is what put it in [`MCP_ONLY_WIRE_ARGUMENTS`]), so
/// only a tool name can be excused, and only by a subcommand `main.rs` really dispatches.
fn mcp_token_has_named_twin(token: &str, norm: &str, subcommands: &[String]) -> bool {
    if MCP_ONLY_WIRE_ARGUMENTS.contains(&token) {
        return false;
    }
    let sub = token.replace('_', "-");
    subcommands.iter().any(|s| s == &sub) && names_token(norm, &format!("`zzop {sub}"))
}

/// Literals that name a host spelling whose twin DOES NOT EXIST, subtracted by (file, token) — the
/// residue the twin clause above cannot reach, with the reason each one is honest rather than a leak.
///
/// Per-token and per-file, never per-file alone: every OTHER host spelling in these two files is still
/// scanned, so exempting the one lane a message is about does not blind the file it lives in.
///
/// Both entries are the same shape: a document served to BOTH audiences, describing a lane that exists
/// on only one of them, and saying so in the same sentence. Rewording them to be spelling-free would
/// delete the fact the sentence is there to deliver — which host can run this and which cannot.
const CLI_NO_TWIN_EXEMPTIONS: [(&str, &str, &str); 2] = [
    (
        "crates/engine/src/disclosure.rs",
        "`zzop coverage",
        "the sightline census's own text says `the CLI-only `zzop coverage` lane ... (it has no MCP \
         tool twin; an MCP host reads the same declarations out of this document)` — it names the CLI \
         lane in order to tell the MCP reader that the document in their hands is the substitute",
    ),
    (
        "crates/engine/src/disclosure/document.rs",
        "`zzop explain",
        "the disclosure-classes document says `zzop-mcp has no explain — this document, served as MCP \
         resource zzop://contract/disclosure-classes, already carries every id's class, group and \
         status` — the absence of the twin IS the sentence's content",
    ),
];

/// Subtract [`CLI_NO_TWIN_EXEMPTIONS`] from contract 16's `hits`, failing if an entry excused nothing.
///
/// Direction 2 of the exemption discipline (`working-agreements` §5.5): an exemption that no longer
/// matches a real hit is not harmless. The message it excused may have been reworded, or the file
/// renamed — and if a spelling later returns to that file the exemption is already sitting there
/// waiting to swallow it, with nobody having re-read the reason. So a stale entry is a failure, not a
/// no-op.
///
/// CLI direction only, because that is the only direction that has ever needed one: an MCP tool with no
/// CLI twin is a `_cliOnlyLanes` question, already answered by the file-level subtraction contract 16
/// applies before this. A mirror for contract 15 goes here if one is ever earned.
fn subtract_cli_no_twin_exemptions(hits: Vec<Hit>) -> Vec<Hit> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (rel, token, reason) in CLI_NO_TWIN_EXEMPTIONS {
        assert!(
            root.join(rel).is_file(),
            "CLI_NO_TWIN_EXEMPTIONS names {rel} ({reason}), which is not a file — an exemption \
             must not outlive the source it excuses"
        );
        assert!(
            hits.iter().any(|h| h.rel == rel && h.token == token),
            "CLI_NO_TWIN_EXEMPTIONS excuses `{token}` in {rel} ({reason}), but no literal there \
             names it any more. Delete the entry: an exemption left behind after its message was \
             reworded is a pre-armed hole, silently excusing whatever spelling lands in that file next."
        );
    }
    hits.into_iter()
        .filter(|h| {
            !CLI_NO_TWIN_EXEMPTIONS
                .iter()
                .any(|(rel, token, _)| *rel == h.rel && *token == h.token)
        })
        .collect()
}

/// Which of `needles` each prose literal names WITHOUT naming that spelling's twin.
fn offenders_naming(
    prose: &[(PathBuf, usize, String)],
    needles: &[String],
    twin_named: impl Fn(&str, &str) -> bool,
) -> Vec<Hit> {
    let mut out = Vec::new();
    for (path, line, literal) in prose {
        let norm = collapse_whitespace(literal);
        for token in needles {
            if !names_token(&norm, token) || twin_named(token, &norm) {
                continue;
            }
            out.push(Hit {
                rel: rel_path(path),
                line: *line,
                token: token.clone(),
                literal: literal.clone(),
            });
        }
    }
    out
}

/// One offender line per hit.
fn render_offenders(hits: &[Hit], audience: &str) -> Vec<String> {
    hits.iter()
        .map(|h| {
            format!(
                "{}:{}: user-facing string names {audience} `{}` with no twin: {:?}",
                h.rel, h.line, h.token, h.literal
            )
        })
        .collect()
}

/// Does this literal NAME the token, as opposed to merely containing its letters? Word-boundary
/// matching, because a bare substring scan produced a real false positive on the first run: the
/// `config-surface` contract description says `configPaths` (a section of that JSON document, plural),
/// which contains `configPath` and has nothing to do with the MCP argument.
///
/// `-` counts as a word character, which matters only for the derived subcommand needles and was
/// measured the day `crates/engine` entered the haystack: ``` `zzop analyze ``` otherwise matched inside
/// ``` `zzop analyze-envelope ```, reporting the wrong — and twinless — subcommand for a spelling that
/// is correctly paired with the `analyze_envelope` tool right next to it.
fn names_token(literal: &str, token: &str) -> bool {
    let boundary =
        |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-');
    let mut from = 0;
    while let Some(at) = literal[from..].find(token) {
        let start = from + at;
        let end = start + token.len();
        if boundary(literal[..start].chars().next_back()) && boundary(literal[end..].chars().next())
        {
            return true;
        }
        from = start + 1;
    }
    false
}

/// Every double-quoted literal in a whole source file that looks like PROSE (contains a space), paired
/// with its 1-based starting line.
///
/// Scans the FILE, not a line at a time, and that is the load-bearing part: this repo's messages are
/// long enough to wrap, so most of them are `\`-continued across physical lines. A per-line scanner —
/// which is what the first draft of this contract was — reads such a literal's second line as "an
/// unterminated quote" and silently sees nothing, which the invalidation check caught immediately: a
/// deliberately re-planted `check_endpoint` in `cross.rs`'s wrapped edge-cap hint went UNDETECTED while
/// the same plant on a one-line message in `trees.rs` went red.
///
/// Also skipped: `//` comments (a doc comment naming the real tool a function backs is how the source
/// stays navigable — see this contract's own doc) and `'x'` char literals (a `'"'` in the scanner code
/// of a future sibling would otherwise open a literal that swallows the rest of the file). Escaped
/// quotes (`\"`, which the shipped messages really do contain) do not terminate a literal.
///
/// RAW STRINGS are not understood as such, and the consequence is worth stating exactly rather than
/// waving at, because one is in the haystack (`crates/config/src/template.rs`'s starter config): the `r#`
/// prefix is skipped, the opening `"` starts a literal, and every `"` INSIDE the raw text ends one. So a
/// raw string is scanned as a run of FRAGMENTS rather than one literal. That is lossy but fail-safe in
/// the direction that matters — a needle is missed only if it straddles an embedded quote, and no
/// fragment is invented — which is why it is left alone rather than fixed with a raw-string parser this
/// suite would then have to test.
fn prose_literals(source: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line) = (0usize, 1usize);
    while i < chars.len() {
        match chars[i] {
            '\n' => {
                line += 1;
                i += 1;
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            // `'x'` / `'\n'` — a char literal, never a string. A lifetime (`'a`) has no closing quote
            // in that position, so it falls through to the single-character advance below.
            '\'' if chars.get(i + 2) == Some(&'\'') => i += 3,
            '\'' if chars.get(i + 1) == Some(&'\\') && chars.get(i + 3) == Some(&'\'') => i += 4,
            '"' => {
                let start_line = line;
                let mut literal = String::new();
                let mut j = i + 1;
                while j < chars.len() {
                    if chars[j] == '\\' {
                        if let Some(&next) = chars.get(j + 1) {
                            if next == '\n' {
                                line += 1;
                            }
                            literal.push(next);
                        }
                        j += 2;
                        continue;
                    }
                    if chars[j] == '"' {
                        break;
                    }
                    if chars[j] == '\n' {
                        line += 1;
                    }
                    literal.push(chars[j]);
                    j += 1;
                }
                if j >= chars.len() {
                    break;
                }
                if literal.contains(' ') {
                    out.push((start_line, literal));
                }
                i = j + 1;
            }
            _ => i += 1,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    /// Seals the extractor the contract stands on: a prose literal is picked up with its line, an
    /// identifier-shaped one is not, an escaped quote does not cut a literal short (the shipped `trees`
    /// messages contain `\"auto\"`, so this case is real), a `//` comment is invisible, and — the case
    /// the invalidation check exposed — a `\`-CONTINUED literal is read whole across physical lines.
    #[test]
    fn prose_literals_takes_sentences_and_skips_identifiers() {
        let texts = |s: &str| -> Vec<String> {
            super::prose_literals(s)
                .into_iter()
                .map(|(_, t)| t)
                .collect()
        };
        assert_eq!(
            texts(r#"Err("use analyze_repo for it".to_string())"#),
            vec!["use analyze_repo for it".to_string()]
        );
        assert!(texts(r#"get("configPath")"#).is_empty());
        assert!(texts(r#"// doc: the check_endpoint tool answers this"#).is_empty());
        assert_eq!(
            texts(r#"f("declare `trees` (2+, or \"auto\") now")"#),
            vec![r#"declare `trees` (2+, or "auto") now"#.to_string()]
        );
        // A wrapped message: the token sits on the SECOND physical line and must still be seen, with
        // the literal's own STARTING line reported.
        let wrapped = "let m = \"drill into a route \\\n    with check_endpoint\";";
        let found = super::prose_literals(wrapped);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0, 1, "reports the literal's starting line");
        assert!(found[0].1.contains("check_endpoint"), "{found:?}");
        // Word boundaries: `configPaths` (a section of the config-surface document) is not the MCP
        // argument `configPath`, and a bare substring scan called it one on this test's first run.
        assert!(super::names_token("pass configPath here", "configPath"));
        assert!(!super::names_token("the configPaths section", "configPath"));
        // `-` is a word character: `zzop analyze` must not be reported inside `zzop analyze-envelope`,
        // which is a different subcommand with a different twin.
        assert!(!super::names_token(
            "run `zzop analyze-envelope <f>`",
            "`zzop analyze"
        ));
        assert!(super::names_token("run `zzop analyze .`", "`zzop analyze"));
    }

    /// Seals the TWIN clause in both directions, including the two ways it must NOT be widened.
    #[test]
    fn a_twin_is_excused_only_when_it_exists_and_is_named_for_that_same_surface() {
        let tools = super::shipped_mcp_tool_names();
        let subs = super::cli_subcommands();
        let twinned = |lit: &str, token: &str| {
            super::cli_token_has_named_twin(token, &super::collapse_whitespace(lit), &tools)
        };

        // Paired, the shape crates/engine's disclosure messages use.
        assert!(twinned(
            "contract: MCP resource `zzop://contract/envelope-guide` on MCP hosts \
             (`zzop contract envelope-guide` with the CLI binary)",
            "`zzop contract"
        ));
        // PER-TOKEN, not per-literal: a literal carrying one correctly twinned document must not
        // excuse a second document that has no twin. This is the live violation the sweep found.
        assert!(!twinned(
            "`zzop contract envelope-guide` / `zzop://contract/envelope-guide`, and \
             `zzop contract adapter-guide` for the on-ramp",
            "`zzop contract"
        ));
        // A subcommand whose twin is a real shipped tool, named.
        assert!(twinned(
            "check it with `zzop validate-envelope <f>` / MCP tool `validate_envelope`",
            "`zzop validate-envelope"
        ));
        // The twin must be NAMED — claiming the pairing in prose is not naming it.
        assert!(!twinned(
            "check it with `zzop validate-envelope <f>` (the MCP host has an equivalent)",
            "`zzop validate-envelope"
        ));
        // The twin must EXIST: `zzop coverage` has no MCP tool, so nothing can excuse it.
        assert!(!twinned(
            "the `zzop coverage` lane and its `coverage` reply",
            "`zzop coverage"
        ));
        // A dash-flag can never be twinned — there is no argv on the wire.
        assert!(!twinned("pass `--severity high` / severity", "--severity"));

        // MCP direction, mirrored.
        let mcp = |lit: &str, token: &str| {
            super::mcp_token_has_named_twin(token, &super::collapse_whitespace(lit), &subs)
        };
        assert!(mcp(
            "`zzop analyze-envelope <f>` / MCP tool `analyze_envelope`",
            "analyze_envelope"
        ));
        // `cross_repo` has no CLI subcommand of that spelling, so the historical leak stays RED.
        assert!(!mcp(
            "use the cross_repo tool, or `zzop cross-repo`",
            "cross_repo"
        ));
        // A wire argument has no CLI twin by construction.
        assert!(!mcp("pass configPath / `zzop config-path`", "configPath"));
    }

    /// The `\`-continuation the twin clause has to survive: the document name lands on the next
    /// physical line, indented, and must still read as one token.
    #[test]
    fn collapse_whitespace_rejoins_a_wrapped_command() {
        let wrapped = "see `zzop contract \r\n         adapter-guide` for it";
        assert_eq!(
            super::contract_documents_named(&super::collapse_whitespace(wrapped)),
            vec!["adapter-guide".to_string()]
        );
        assert!(super::contract_documents_named("no command here").is_empty());
    }
}
