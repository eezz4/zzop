//! `zzop-parser-go` — a `tree-sitter-go`-based Go parser frontend -> Common IR projection, mirroring
//! `zzop-parser-rust`'s (itself mirroring `zzop-parser-python-3`'s) crate shape and discipline: grammar
//! AST types stay inside this crate (a `tree-sitter-go` upgrade should never leak into the public IR);
//! only `zzop_core` types cross the crate boundary — enforced by the same isolation discipline
//! `scripts/check-syn-isolation.sh` pins for `syn` (a future `check-tree-sitter-isolation.sh` sibling
//! script covers this crate; see this crate's own public API below for the enforced surface).
//!
//! ## Layout
//! - `lang` — CST -> Common-IR LANGUAGE projection: `SourceSymbol` extraction (`symbols`), `ImportMap`
//!   extraction (`imports`), identifier-reference collection (`used_names`), and the pure
//!   import-path -> package-directory resolver (`resolve`).
//! - `adapters` — framework-vocabulary producers emitting cross-layer IO facts: `net/http` (raw
//!   `DefaultServeMux`/`http.NewServeMux`) and `gin` router PROVIDES as router-mount fragments
//!   (`adapters::net_http`, `adapters::gin`, combined by `extract_go_router_fragments`), and `net/http`
//!   client literal HTTP egress CONSUMES (`adapters::http_clients`).
//!
//! ## Tree-sitter discipline (this is the first tree-sitter-based parser in the workspace — this
//! crate sets the pattern every future one follows)
//! - **Parse once per public fn call.** Every `pub fn` below parses `text` exactly once via
//!   [`parse_tree`], then walks the resulting `tree_sitter::Tree` with typed-by-node-kind matching
//!   (`Node::kind()` compared against a literal grammar node-kind string) — never a second parse
//!   inside the same call. Each public fn still parses INDEPENDENTLY of its sibling public fns (the
//!   same "each function parses internally" contract `zzop_parser_rust`/`zzop_parser_python_3` uphold
//!   for `zzop-engine`'s per-fact caching granularity), so calling `parse_go` and then
//!   `extract_go_router_fragments` on the same `text` parses twice total — once each.
//! - **Never-guess on parse errors.** [`parse_tree`] returns `None` when the ROOT node itself is an
//!   error (`Node::is_error()`) — total parse failure, e.g. binary garbage or a source file with no
//!   recognizable Go at all — and every public fn here degrades accordingly (`parse_go` -> `None`,
//!   every `Vec`-returning fn -> empty). A PARTIAL error elsewhere (one bad statement inside an
//!   otherwise-valid function) does NOT propagate: every recursive walk in this crate checks
//!   `Node::is_error()` / `Node::is_missing()` at EVERY node it visits and skips just that subtree,
//!   continuing with its siblings — see `lang::symbols`/`lang::used_names`/`adapters::*`'s own module
//!   docs for the walk shape. This is what "extract from the valid regions only" means throughout this
//!   crate: a single malformed statement never blanks out the rest of an otherwise-fine file.
//! - **Node-kind vocabulary is pinned.** Every grammar node-kind string this crate matches on (e.g.
//!   `"function_declaration"`, `"call_expression"`, `"selector_expression"`) is enumerated in
//!   `node_kinds`'s `node_kinds_are_pinned_to_the_grammar` test, which asserts each one is a REAL kind
//!   in the compiled `tree_sitter_go::LANGUAGE` (`Language::id_for_node_kind(kind, true) != 0`). A
//!   grammar upgrade that renames a kind fails THIS test with a clear diff, instead of every extractor
//!   silently returning nothing.
//! - **No tree-sitter types in the public API.** Every `pub fn`/`pub type` below is built from
//!   `zzop_core`/`std` types only — `tree_sitter::{Tree, Node, Language, ...}` never appears in this
//!   crate's public signatures, so a `tree-sitter`/`tree-sitter-go` version bump can never become a
//!   breaking change for a caller.

pub mod adapters;
pub mod lang;
// Test-only: `node_kinds::PINNED_NODE_KINDS` has no runtime purpose outside its own pin test (crate
// root doc's "node-kind vocabulary is pinned" section) — `cfg(test)`-gating the whole module (rather
// than `#[allow(dead_code)]`-ing the const) keeps a real "unused" finding from ever being silenced here.
#[cfg(test)]
mod node_kinds;
mod util;

pub use adapters::extract_go_router_fragments;
pub use adapters::gorm::{extract_gorm_db_table_consumes, extract_gorm_db_table_provides};
pub use adapters::http_clients::extract_go_http_consumes;
pub use lang::imports::parse_imports;
pub use lang::loop_spans::extract_loop_spans;
pub use lang::resolve::go_package_dir_of;
pub use lang::symbols::parse_symbols;
pub use lang::used_names::parse_local_identifier_refs;

/// Cache-bust token for `zzop-cache`: `parser-id/pinned-toolchain/last-change-version`. The
/// `tree-sitter-go` segment must match this crate's `Cargo.toml` pin (a grammar upgrade changes
/// extraction → restamp); the trailing `CARGO_PKG_VERSION` is restamped when this crate's projected IR
/// shape changes, else kept so warm Go caches survive the upgrade (2026-07-22 version reform).
pub const PARSER_FINGERPRINT: &str = "go/tree-sitter-go-0.25.0/0.21.0";

/// Every top-level declaration kind `lang::symbols`/`lang::imports` recognize, PLUS `package_clause`
/// (never itself extracted, but still a sign the file has SOME real Go in it) — `parse_tree`'s
/// "root itself fails to parse" gate below asks "is there at least one of these among the root's own
/// top-level children?", not "is the root node's OWN kind literally `ERROR`" (empirically, tree-sitter-go
/// almost never assigns the ROOT itself kind `ERROR` — a permissive `source_file: repeat($._toplevel)`
/// grammar rule instead absorbs unparseable input into `ERROR`/`MISSING` CHILDREN while the root stays
/// kind `source_file`; see `node_kinds::tests` for the pinned kinds these are drawn from).
const TOP_LEVEL_DECLARATION_KINDS: &[&str] = &[
    "package_clause",
    "import_declaration",
    "function_declaration",
    "method_declaration",
    "type_declaration",
    "const_declaration",
    "var_declaration",
];

/// Parses `text` with `tree-sitter-go`, returning `None` when the root "fails to parse" — either
/// `Node::is_error()` on the root directly (the raw grammar-level signal), or, since that signal
/// almost never fires in practice (doc above), when NONE of the root's own top-level children survive
/// as a recognized, non-error/non-missing declaration kind (`TOP_LEVEL_DECLARATION_KINDS`) — i.e. the
/// file has nothing usable at all. A file with at least ONE valid top-level declaration alongside
/// broken ones still returns `Some` — module doc's "never-guess on parse errors": a partial error
/// elsewhere must not blank out an otherwise-fine file. The caller falls back to a lexical scan on
/// `None`, the same contract every parser in this workspace upholds for a parse failure.
///
/// Known parity deviation (deliberate, opus review F4): a COMMENT-ONLY `.go` file hits the second
/// gate (its named children are all `comment` nodes, none a declaration) and is reported degraded,
/// whereas TS/Python/Rust do not degrade a comment-only file. Accepted: valid Go requires a `package`
/// clause, so a comment-only `.go` file is not a well-formed compilation unit to begin with, and the
/// only observable difference is the `degraded` flag (such a file carries no symbols/imports either
/// way). An EMPTY file (zero named children) short-circuits the `> 0` guard and is NOT degraded,
/// matching the sibling parsers. Internal-only: `tree_sitter::Tree` never crosses this crate's
/// public API.
pub(crate) fn parse_tree(text: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&go_language()).ok()?;
    let tree = parser.parse(text, None)?;
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

fn go_language() -> tree_sitter::Language {
    tree_sitter_go::LANGUAGE.into()
}

/// Raw physical line count — mirrors `zzop_parser_rust::count_loc`/`zzop_parser_python_3::count_loc`
/// exactly. The file is never parsed here, just counted, so this is safe to call even when
/// `parse_tree` would return `None`.
pub fn count_loc(text: &str) -> u32 {
    text.split('\n').count() as u32
}

/// Language projection: source -> `(symbols, imports, loc, used_names)`, the tuple mirroring
/// `zzop_parser_rust::parse_rust`'s pipeline slot shape. Returns `None` when `parse_tree` fails on
/// `text` — the caller degrades to a lexical fallback. `imports`/`used_names` are computed from a
/// fresh parse each (module doc's "parse once per public fn call" — this function's OWN gate parse is
/// a fourth, separate parse; acceptable duplication for the "each function parses internally" public
/// contract `zzop-engine` relies on for per-fact caching granularity).
pub fn parse_go(
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
    fn parse_go_returns_none_on_hopeless_input() {
        // Binary garbage — no recognizable Go at all, the root node itself is an ERROR.
        assert!(parse_go("bad.go", "\u{0}\u{1}\u{2}\u{3}not go at all{{{{").is_none());
    }

    #[test]
    fn parse_go_returns_some_on_valid_source() {
        let out = parse_go("ok.go", "package main\n\nfunc main() {}\n");
        assert!(out.is_some());
    }

    #[test]
    fn count_loc_matches_workspace_convention() {
        assert_eq!(count_loc("a\nb\n"), 3);
        assert_eq!(count_loc(""), 1);
    }
}
