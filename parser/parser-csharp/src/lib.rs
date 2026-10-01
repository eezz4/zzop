//! `zzop-parser-csharp` — a `tree-sitter-c-sharp`-based C# parser frontend -> Common IR projection,
//! mirroring `zzop-parser-go`'s tree-sitter discipline exactly and `zzop-parser-java-21`'s nested-type/
//! attribute-routing shape (grammar AST types stay inside this crate; only `zzop_core` types cross the
//! crate boundary — enforced by `scripts/check-tree-sitter-isolation.sh`'s allowlist).
//!
//! ## Layout
//! - `lang` — CST -> Common-IR LANGUAGE projection: `SourceSymbol` extraction (`symbols`, top-level +
//!   type-nested classes/interfaces/structs/records/delegates, methods/constructors/properties/consts
//!   with body spans), `ImportMap` extraction (`imports`, `using` directives), identifier-reference
//!   collection (`used_names`), and every namespace this file declares (`namespaces`).
//! - `adapters` — cross-layer IO: ASP.NET Core attribute-routed + minimal-API HTTP route PROVIDES
//!   (`adapters::provides`), `HttpClient` literal HTTP egress CONSUMES (`adapters::http_clients`), and
//!   EF Core `DbSet<T>`/`[Table]` `db-table` PROVIDES (`adapters::ef_core`).
//!
//! ## Tree-sitter discipline (mirrors `zzop_parser_go`'s crate-root doc verbatim — see that crate for
//! the fuller rationale; summarized here)
//! - **Parse once per public fn call.** Every `pub fn` parses `text` exactly once via [`parse_tree`],
//!   then walks the resulting `tree_sitter::Tree`. Sibling public fns each parse independently.
//! - **Never-guess on parse errors.** [`parse_tree`] returns `None` when the root is hopeless (crate
//!   root gate below); a PARTIAL error elsewhere never blanks the rest of an otherwise-valid file —
//!   every walk in this crate skips just the erroring subtree via `util::valid_named_children`.
//! - **Node-kind vocabulary is pinned** — `node_kinds::PINNED_NODE_KINDS` (test-only), asserted against
//!   the compiled `tree_sitter_c_sharp::LANGUAGE`.
//! - **No tree-sitter types in the public API.**

pub mod adapters;
pub mod lang;
mod parse_census;
mod project;
mod util;

use zzop_core::recognizer::{channel, FrameworkRecognizer};

/// Frameworks this parser recognizes — see [`zzop_core::recognizer`]. Verified against return types.
pub const FRAMEWORK_RECOGNIZERS: &[FrameworkRecognizer] = &[
    FrameworkRecognizer {
        framework: "asp.net core",
        extensions: &["cs"],
        emits: &[channel::PROVIDES],
    },
    FrameworkRecognizer {
        framework: "httpclient",
        extensions: &["cs"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "ef core",
        extensions: &["cs"],
        emits: &[channel::DB_PROVIDES],
    },
];

#[cfg(test)]
mod node_kinds;

pub use adapters::ef_core::extract_ef_core_db_table_provides;
pub use adapters::http_clients::extract_csharp_http_consumes;
pub use adapters::provides::{
    extract_csharp_http_provides, CSharpRouteVocab, DEFAULT_ROOT_ROUTE_BUILDER_VARIABLE_NAMES,
};
pub use lang::call_sites::extract_call_sites;
pub use lang::imports::parse_imports;
pub use lang::loop_spans::extract_loop_spans;
pub use lang::namespaces::csharp_namespaces_of;
pub use lang::string_literals::extract_string_literals;
pub use lang::symbols::parse_symbols;
pub use lang::used_names::parse_local_identifier_refs;
pub use parse_census::{parse_count, reset_parse_count};
pub use project::{extract_csharp_http_provides_project, CSharpProjectProvidesReport};

/// Cache-bust token for `zzop-cache`: `parser-id/pinned-toolchain/last-change-version`. The
/// `tree-sitter-c-sharp` segment names this crate's REAL exact pin (`tree-sitter-c-sharp = "=0.23.5"`
/// in `Cargo.toml`), so it stays accurate for whoever reads it — unlike
/// `zzop_parser_typescript`'s caret-range label. Keeping the two in step is a courtesy to that
/// reader, not a correctness duty.
///
/// **This string is an ID, not a version — it no longer has to be bumped.** `crates/engine/build.rs`
/// hashes this crate's whole dependency closure into the cache key beside it, so a change to any
/// source here invalidates on its own. What is left is the part a person reads in a cache path or a
/// bug report: which frontend parsed the file. Change it when the FRONTEND changes; correctness no
/// longer depends on remembering.
pub const PARSER_FINGERPRINT: &str = "csharp/tree-sitter-c-sharp-0.23.5/0.21.0";

/// Every top-level declaration kind this crate recognizes, PLUS `global_statement` (a top-level
/// executable statement — C#'s "top-level program" feature — never itself extracted, but still a sign
/// the file has SOME real C# in it) — the root-hopeless gate's "is there at least one of these among
/// the root's own top-level children?" set. Mirrors `zzop_parser_go::TOP_LEVEL_DECLARATION_KINDS`'s
/// exact role and doc. `namespace_declaration` covers the block form; `file_scoped_namespace_declaration`
/// the C# 10 `namespace X;` form.
const TOP_LEVEL_DECLARATION_KINDS: &[&str] = &[
    "using_directive",
    "namespace_declaration",
    "file_scoped_namespace_declaration",
    "class_declaration",
    "interface_declaration",
    "struct_declaration",
    "enum_declaration",
    "record_declaration",
    "delegate_declaration",
    "global_statement",
];

/// Parses `text` with `tree-sitter-c-sharp`, returning `None` when the root "fails to parse" — either
/// `Node::is_error()` on the root directly, or (the far more common real-world signal, mirroring
/// `zzop_parser_go`/`zzop_parser_java_21::parse_tree`'s identical two-gate shape) when NONE of the
/// root's own top-level children survive as a recognized, non-error/non-missing declaration kind
/// ([`TOP_LEVEL_DECLARATION_KINDS`]). A file with at least ONE valid top-level declaration alongside
/// broken ones still returns `Some` — a partial error elsewhere must not blank out an otherwise-fine
/// file.
///
/// Known parity deviation (deliberate, mirrors `zzop_parser_go`/`zzop_parser_java_21`'s own documented
/// F4 comment-only-file gap): a COMMENT-ONLY `.cs` file hits the second gate (its named children are
/// all `comment`, none a declaration) and is reported degraded, whereas TS/Python/Rust do not degrade a
/// comment-only file. Accepted for the same reason those two crates accept it: the only observable
/// difference is the `degraded` flag (such a file carries no symbols/imports either way), and an EMPTY
/// file (zero named children) short-circuits the `> 0` guard and is NOT degraded, matching every
/// sibling parser. Internal-only: `tree_sitter::Tree` never crosses this crate's public API.
pub(crate) fn parse_tree(text: &str) -> Option<tree_sitter::Tree> {
    parse_tree_memo(text)
}

/// ONE-SLOT, THREAD-LOCAL MEMO of the most recent parse.
///
/// # Why this exists (2026-09-25, review ledger V402)
/// This frontend extracts each fact with its own walk, and every walk started by re-parsing the same
/// bytes — `parse_csharp`'s own gate comment said so out loud ("each sub-call below re-parses
/// independently") and there are eleven non-test `parse_tree(text)` call sites. MEASURED cold on
/// `corpus/frameworks/aspnetcore` (10,740 `.cs` files): **141,358 parses, 13.16 per file.**
///
/// # What it is worth, measured rather than assumed
/// Parse time (summed across rayon workers) **201.04s -> 43.96s, a 4.6x cut**; wall clock **490s ->
/// 469s**. 🔴 Those two are not the same claim and the first does not imply the second: 201s of
/// thread-time over ~10 workers is ~20s of wall, which is exactly the 21s observed. The honest
/// headline is **4.3% off the wall of the largest tree in the corpus**, plus 157 seconds of CPU that
/// nothing needed to do. On `corpus/audit/eShop` (544 files, cold, two runs each): 3.45s/3.50s ->
/// 0.69s/0.70s of parse time with byte-identical findings.
///
/// # Why one slot, and why bytes rather than a hash
/// One slot is enough because a file's walks are consecutive within one thread's work on it; a second
/// slot would buy nothing and cost memory per worker. The key is the TEXT ITSELF, compared byte for
/// byte — a 64-bit hash collision here would hand a walk the wrong file's tree, and "two strings whose
/// hashes match are the same string" is a guess. Holding one file's source per thread is a few KB
/// against a parse that costs ~1.4 ms, and `Tree` is refcounted internally so handing the same tree to
/// thirteen walks is free.
fn parse_tree_memo(text: &str) -> Option<tree_sitter::Tree> {
    use std::cell::RefCell;

    thread_local! {
        static LAST: RefCell<Option<(String, Option<tree_sitter::Tree>)>> =
            const { RefCell::new(None) };
    }

    if let Some(hit) = LAST.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|(seen, _)| seen == text)
            .map(|(_, tree)| tree.clone())
    }) {
        return hit;
    }

    let fresh = parse_tree_inner(text);
    LAST.with(|slot| *slot.borrow_mut() = Some((text.to_string(), fresh.clone())));
    fresh
}

fn parse_tree_inner(text: &str) -> Option<tree_sitter::Tree> {
    // Counted, and NOT memoized here on purpose — the memo is one level up, in `parse_tree_memo`,
    // which is the only caller of this function. Sixteen call sites in this crate ask for the same
    // text in a row; before 2026-09-25 each paid a full parse (fourteen per file, measured), and the
    // memo cut that to three. The count is what it buys: the wall clock did not move. Keeping the
    // counter INSIDE this function rather than inside the memo is what makes that true — a census
    // that counted memo hits would report three while the parser still did fourteen.
    // `parse_census`'s module doc is the owner of the A/B and of the condition under which the memo
    // would come back out (review ledger V116, then V402).
    parse_census::record_parse();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&csharp_language()).ok()?;
    let tree = util::parse_within_depth(&mut parser, text)?;
    let root = tree.root_node();
    if root.is_error() {
        return None;
    }
    if root.named_child_count() > 0
        && !util::valid_named_children(root)
            .iter()
            .any(|c| TOP_LEVEL_DECLARATION_KINDS.contains(&c.kind()))
    {
        return None;
    }
    Some(tree)
}

fn csharp_language() -> tree_sitter::Language {
    tree_sitter_c_sharp::LANGUAGE.into()
}

/// Raw physical line count — mirrors every other parser crate's `count_loc` exactly. The file is never
/// parsed here, so this is safe to call even when [`parse_tree`] would return `None`.
pub fn count_loc(text: &str) -> u32 {
    text.split('\n').count() as u32
}

/// Language projection: source -> `(symbols, imports, loc, used_names)`, the tuple mirroring
/// `zzop_parser_go::parse_go`/`zzop_parser_java_21::parse_java`'s pipeline slot shape. Returns `None`
/// when `parse_tree` fails on `text` — the caller degrades to a lexical fallback.
pub fn parse_csharp(
    rel: &str,
    text: &str,
) -> Option<(
    Vec<zzop_core::SourceSymbol>,
    zzop_core::ImportMap,
    u32,
    Vec<String>,
)> {
    parse_tree(text)?; // parse-failure gate only — each sub-call below re-parses independently.
    let symbols = lang::symbols::parse_symbols(rel, text);
    let imports = lang::imports::parse_imports(text);
    let loc = count_loc(text);
    let used_names: Vec<String> = lang::used_names::parse_local_identifier_refs(text)
        .into_iter()
        .collect();
    Some((symbols, imports, loc, used_names))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csharp_returns_none_on_hopeless_input() {
        assert!(parse_csharp("bad.cs", "\u{0}\u{1}\u{2}\u{3}not csharp at all{{{{").is_none());
    }

    #[test]
    fn parse_csharp_returns_some_on_valid_source() {
        let out = parse_csharp("Ok.cs", "class Ok { void M() {} }\n");
        assert!(out.is_some());
    }

    #[test]
    fn parse_csharp_returns_none_on_comment_only_file_documented_deviation() {
        // Known parity deviation with TS/Python/Rust, mirrors zzop_parser_go/java's own F4 gap.
        assert!(parse_csharp("c.cs", "// just a comment\n").is_none());
    }

    #[test]
    fn parse_csharp_returns_some_on_empty_file() {
        assert!(parse_csharp("empty.cs", "").is_some());
    }

    #[test]
    fn count_loc_matches_workspace_convention() {
        assert_eq!(count_loc("a\nb\n"), 3);
        assert_eq!(count_loc(""), 1);
    }
}
