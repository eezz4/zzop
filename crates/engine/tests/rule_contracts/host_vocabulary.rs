//! Contracts 15 and 16: the SHARED crates are host-vocabulary-free — a message `zzop-summary`/
//! `zzop-config` build must not spell EITHER host's vocabulary, because the other host's user reads the
//! same sentence and cannot act on it. 15 is the MCP direction, 16 the CLI direction; they are one
//! doctrine scanned twice, and split into two tests only so a failure names which audience was ignored.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::collect_rs_files;
use crate::surface_parity::{cli_only_lane_sources, load_registry};

/// The crates both products call through: `zzop-summary` (every shaping entry point), `zzop-config`
/// (config loading + request assembly), and `zzop-facade` (the analysis entry points both of those sit
/// on top of). Every string built in one of these can surface on the `zzop` CLI's stdout/stderr AND in a
/// `zzop-mcp` tool reply.
///
/// `zzop-facade` was NOT in this list when the contract was written, and the justification given for
/// leaving it out — "its text is addressed to a direct embedder, and the audit found no tool name in it"
/// — was false on the day it was written: `analyze.rs`'s non-directory-root error told a CLI user that
/// an envelope JSON is `validate_envelope`'s input, a spelling that exists on no CLI. It reproduced on
/// the real binary. The lesson is about scope prose, not about that one message: a guard whose stated
/// boundary claims an audit found nothing is indistinguishable from one that never looked, and it is the
/// sentence a later reader trusts instead of re-checking. State the boundary MECHANICALLY (what is in the
/// list) and let the list be the claim.
///
/// `zzop-core` stays out for a reason that is checkable rather than historical: it is the vocabulary-free
/// kernel, and contract 8 (`kernel_vocabulary.rs`) already forbids it from naming even a RULE id — a host
/// name is strictly further out of bounds than what that contract already blocks.
fn shared_src_dirs() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    vec![
        root.join("crates/summary/src"),
        root.join("crates/config/src"),
        root.join("crates/facade/src"),
    ]
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
    let offenders = offenders_naming(&prose, &needles, "MCP-only");
    assert!(
        offenders.is_empty(),
        "a shared crate's user-facing message names MCP-only vocabulary — the SAME sentence reaches a \
         `zzop` CLI user, who cannot call a tool or pass a JSON argument. Make it spelling-free, or take \
         the word from a caller-supplied parameter (see zzop_config::trees's `operation`): {offenders:#?}"
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
    const MAIN: &str = include_str!("../../../../packages/cli-bin/src/main.rs");
    let mut out: Vec<String> = Vec::new();
    let mut rest = MAIN;
    while let Some(i) = rest.find("Some(\"") {
        rest = &rest[i + 6..];
        let Some(end) = rest.find('"') else { break };
        let name = &rest[..end];
        if !name.starts_with('-') && name != "help" {
            // BACKTICKED, because that is how this repo writes a command and prose is how it
            // writes a word. Deriving bare `zzop <sub>` matched "a key from a different zzop version"
            // — real prose in an unknown-key warning — on the first run. The old hand list omitted
            // `version`/`file`/`diff`/`graph` for exactly this reason, which read as incompleteness
            // and was partly deliberate; the backtick keeps the completeness without the collision.
            out.push(format!("`zzop {name}"));
        }
    }
    let subcommands = out.len();
    assert!(
        subcommands > 5,
        "extracted {subcommands} subcommand(s) from main.rs — the dispatch parse has stopped matching, \
         and a needle list that shrinks silently is what this derivation replaced"
    );

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
    let offenders = offenders_naming(&prose, &needles, "CLI-only");
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
    }
    (scanned, prose)
}

/// The EMPTY-NEEDLE guard both contracts stand on.
///
/// Without it, an empty needle list makes these tests walk the entire shared-crate haystack, find
/// nothing to look for, and report a green — the "a guard over an empty subject set always passes"
/// failure mode this repo has now been bitten by three times. It matters in different ways on the two
/// sides: contract 15's list is DERIVED, so a broken extraction empties it silently, and contract 16's
/// is hand-written, so a bad edit can. Neither can now go quiet.
fn assert_needles_non_empty(needles: &[String], audience: &str) {
    assert!(
        !needles.is_empty(),
        "the {audience} needle list is EMPTY — this contract would then scan every shared-crate prose \
         literal looking for nothing and pass vacuously"
    );
}

/// Which of `needles` each prose literal names, formatted as one offender line per hit.
fn offenders_naming(
    prose: &[(PathBuf, usize, String)],
    needles: &[String],
    audience: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    for (path, lineno, literal) in prose {
        for token in needles {
            if names_token(literal, token) {
                out.push(format!(
                    "{}:{lineno}: user-facing string names {audience} `{token}`: {literal:?}",
                    path.display(),
                ));
            }
        }
    }
    out
}

/// Does this literal NAME the token, as opposed to merely containing its letters? Word-boundary
/// matching, because a bare substring scan produced a real false positive on the first run: the
/// `config-surface` contract description says `configPaths` (a section of that JSON document, plural),
/// which contains `configPath` and has nothing to do with the MCP argument.
fn names_token(literal: &str, token: &str) -> bool {
    let boundary = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_');
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
    }
}
