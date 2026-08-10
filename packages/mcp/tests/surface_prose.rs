//! Prose-drift pin between the `zzop` CLI's hand-written USAGE string (package `zzop-cli-bin`,
//! `packages/cli-bin/src/main.rs` — read here by relative path, since this repo's Cargo packages are
//! plain sibling directories, not published crates) and this crate's own `tools/definitions.rs` tool
//! schemas — two hand-written descriptions of one argument contract, now living in two different
//! Cargo packages. Runtime answers can't drift apart (both the CLI subcommand and its MCP tool twin
//! call the same shared `zzop_summary` functions), but the PROSE describing
//! that contract is free-standing English on both sides and can silently go stale (a renamed tool, a
//! dropped mode, a schema argument with no CLI-side mention). Same drift class `tools/tests.rs`'s
//! `every_tool_name_from_tools_list_appears_in_the_readme` pins for the README table; this pins it
//! for the CLI's own USAGE line instead.
//!
//! Reads the CLI's `USAGE` const directly out of the `zzop-cli-bin` package's source (not by spawning
//! the binary and parsing `--help`) — the const itself, plus its neighboring per-subcommand doc
//! comments, are the literal "prose" half of the contract; the tool schemas from this crate's own
//! `zzop_mcp::tools::list()` are the other half.

const BIN_SOURCE: &str = include_str!("../../cli-bin/src/main.rs");

/// Pulls the usage string literal out of the CLI's main.rs source. A plain substring search over
/// `BIN_SOURCE` (below) would also match the module doc comment's own worked-example lines (5-14), so
/// tests that care about the usage string specifically (as opposed to "does this drift class matter
/// anywhere in the file") isolate it first.
///
/// # Why the anchor is the string's own first bytes
/// This used to key on `const USAGE: &str = "`, which is the DECLARATION rather than the prose, and it
/// broke on 2026-08-06 when the usage line stopped being a const: the graph domain list had to be
/// derived from `GraphDomain::WIRE_NAMES` instead of spelled by hand, which makes it a `format!` inside
/// a function. Nothing about the contract this test pins had changed. Anchoring on `"usage: zzop <` —
/// the literal's own opening bytes — survives any spelling of the item that holds it, and still fails
/// loudly if the prose itself is renamed, which is the only rename this test should care about.
///
/// The extracted text may contain `{}` placeholders. That is harmless here and deliberately not
/// interpolated: every subject below is a substring check for a subcommand phrase or a mode word, none
/// of which live inside a placeholder. A test that needs the RENDERED line must spawn the binary — this
/// one is about the prose a maintainer edits.
fn usage_line() -> &'static str {
    const MARKER: &str = "\"usage: zzop <";
    let start = BIN_SOURCE.find(MARKER).expect(
        "the usage string literal (`\"usage: zzop <...`) was not found in \
         packages/cli-bin/src/main.rs — did the prose get renamed, or did it leave this file?",
    );
    // Past the opening quote, up to the closing one. The literal contains no escaped quotes (it is a
    // flat one-line argument grammar), so the next `"` ends it.
    let after = &BIN_SOURCE[start + 1..];
    let end = after
        .find('"')
        .expect("the usage string literal is not terminated in packages/cli-bin/src/main.rs");
    &after[..end]
}

/// Every MCP tool name's CLI twin subcommand must appear in the CLI's one-line USAGE string. Cheap
/// and loose by design: a substring check, not a full grammar parse — the goal is "a tool the USAGE
/// line never mentions (or a stale rename on either side) fails the build," not byte-parity.
#[test]
fn every_mcp_tool_names_cli_twin_subcommand_appears_in_usage() {
    let usage = usage_line();
    let list = zzop_mcp::tools::list();
    let tool_names: Vec<&str> = list["tools"]
        .as_array()
        .expect("tools/list must return a tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("tool name must be a string"))
        .collect();

    // (MCP tool name, CLI twin subcommand) — a LOOKUP, not the subject set. The distinction is the
    // whole correctness of this test and it was wrong until 2026-07-28: the loop below used to iterate
    // this table, so a tool absent from it was never checked at all. `check_file` shipped in D16 and
    // nobody added a row, and this test stayed GREEN while the USAGE line it exists to pin had no
    // mention of `zzop file` — the guard built for exactly that drift was blind to it. Iterating the
    // SHIPPED tool list instead makes an unpaired tool a failure, so the table cannot silently shrink
    // out from under the check.
    //
    // The CLI spelling is a distinctive USAGE PHRASE rather than the bare subcommand token wherever one
    // token is a substring of another: `analyze` occurs inside `analyze-envelope`, so a bare search
    // would stay green even if the standalone subcommand were dropped.
    let pairs = [
        ("analyze_repo", "analyze <path>"),
        ("cross_repo", "cross"),
        ("check_file", "file <path>"),
        ("check_endpoint", "endpoint"),
        ("analyze_envelope", "analyze-envelope"),
        ("validate_envelope", "validate-envelope"),
        ("validate_rule_pack", "validate-rule-pack"),
    ];

    assert!(
        !tool_names.is_empty(),
        "tools/list returned no tools — this test would then vouch for nothing"
    );

    for tool_name in &tool_names {
        let Some((_, cli_subcommand)) = pairs.iter().find(|(t, _)| t == tool_name) else {
            panic!(
                "MCP tool `{tool_name}` has no row in this test's CLI-twin pairing table. Add one \
                 (with a distinctive USAGE phrase, not a bare token that another subcommand contains) \
                 — or, if this tool deliberately has NO CLI twin, say so here in a row of its own \
                 rather than leaving it unlisted, because unlisted reads as unchecked."
            );
        };
        assert!(
            usage.contains(cli_subcommand),
            "the CLI's USAGE string is missing `{cli_subcommand}`, the CLI twin of MCP tool \
             `{tool_name}` — USAGE: {usage}"
        );
    }
}

/// Mode-vocabulary agreement: `check_endpoint`'s schema offers three mutually exclusive argument
/// names (`path` / `paths` / `configPath`, its `oneOf`) for choosing what to analyze. The CLI
/// expresses the same three modes as one positional form (`<path>...`, covering both the
/// single-path and multi-path schema arguments) and one flag form (`--config <path>`, the schema's
/// `configPath`). This pins the loose pairing — schema keeps all three argument names, and the
/// USAGE line keeps both surface forms — tight enough that dropping a mode on either side breaks
/// it, loose enough to survive rewording.
#[test]
fn check_endpoint_mode_vocabulary_agrees_between_schema_and_cli_usage() {
    let list = zzop_mcp::tools::list();
    let tool = list["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .find(|t| t["name"] == "check_endpoint")
        .expect("check_endpoint tool must be in tools/list");
    let props = &tool["inputSchema"]["properties"];

    // Schema side: all three mode-selecting argument names must still exist.
    assert!(
        props.get("path").is_some(),
        "check_endpoint schema must keep its single-tree `path` mode"
    );
    assert!(
        props.get("paths").is_some(),
        "check_endpoint schema must keep its multi-root `paths` mode"
    );
    assert!(
        props.get("configPath").is_some(),
        "check_endpoint schema must keep its config-first `configPath` mode — the CLI's `--config` \
         counterpart"
    );

    // CLI side: the endpoint usage forms covering path/paths (one positional form) and configPath
    // (--config) must both still appear in USAGE.
    let usage = usage_line();
    assert!(
        usage.contains("endpoint <pattern> <path>..."),
        "USAGE must show endpoint's positional path(s) form (the CLI counterpart of the schema's \
         `path`/`paths` modes): {usage}"
    );
    assert!(
        usage.contains("endpoint <pattern> --config <path>"),
        "USAGE must show endpoint's --config form (the CLI counterpart of the schema's \
         `configPath` mode): {usage}"
    );
}
