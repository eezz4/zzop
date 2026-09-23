//! Small helpers shared across `lang` and `adapters` — kept in one place so the "how do we read a
//! node's text / decide export-ness / compute a 1-based line" primitives are defined exactly once,
//! mirroring how `zzop_parser_rust`'s crate root centralizes `line_of` for the same reason.

use tree_sitter::Node;

/// 1-based line of any tree-sitter node — `node.start_position().row` is 0-based; the task-level
/// contract (and every other parser in this workspace) reports 1-based lines.
pub(crate) fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

/// 1-based END line of any tree-sitter node — `lang::loop_spans`'s inclusive `(start, end)` span pairs
/// need this alongside [`line_of`]'s start-line convention. `node.end_position().row` is the 0-based
/// row of the position immediately AFTER the node's last byte; tree-sitter only advances the row once
/// a `\n` byte has actually been consumed, so for any node whose last byte isn't itself a newline (true
/// of every node this crate spans — a `}`, an identifier, ...) `end_position().row` is exactly the row
/// of that last byte, i.e. this returns the same 1-based line `line_of` would if called on the node's
/// own last character.
pub(crate) fn end_line_of(node: Node) -> u32 {
    node.end_position().row as u32 + 1
}

/// The verbatim source text spanning `node`, empty on any UTF-8 boundary failure (never panics —
/// tree-sitter guarantees valid byte ranges for a well-formed tree, but a defensive empty string
/// keeps this infallible for callers).
pub(crate) fn node_text<'a>(node: Node, src: &'a str) -> &'a str {
    node.utf8_text(src.as_bytes()).unwrap_or("")
}

/// The VERBATIM text of a Go string literal node (`interpreted_string_literal` or
/// `raw_string_literal`): the node's own span includes its delimiter (`"..."` or `` `...` ``, both
/// exactly one byte each), so stripping the first/last byte yields the interior text. Deliberately
/// NOT escape-sequence decoding (`\n` stays the two literal characters `\` `n`) — full decoding
/// would mean walking the node's `escape_sequence`/`*_content` children, and no caller needs it:
/// the import-path and HTTP-literal readers consume spellings that never plausibly carry an escape,
/// and the one ESCAPE-SENSITIVE caller (`lang::string_literals`, which hashes arbitrary credential
/// values) applies its own gate ON TOP — an interpreted literal containing `\` is SILENCED there
/// rather than decoded, so this helper's verbatim contract stays a single, simple thing. `None` for
/// any other node kind (never guessed).
pub(crate) fn string_literal_text(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "interpreted_string_literal" | "raw_string_literal" => {
            let raw = node_text(node, src);
            if raw.len() < 2 {
                return Some(String::new());
            }
            Some(raw[1..raw.len() - 1].to_string())
        }
        _ => None,
    }
}

/// `node`'s own NAMED children, skipping any that are themselves an error/missing subtree — the
/// shared "extract from the valid regions only" filter every top-level/grouped-declaration walk in
/// `lang::symbols`/`lang::imports` applies before matching on `Node::kind()`. Collected into a `Vec`
/// (nodes are `Copy`) rather than returned as a lazy iterator, so a caller never has to juggle the
/// `TreeCursor` borrow this needs internally.
pub(crate) fn valid_named_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|c| !c.is_error() && !c.is_missing())
        .collect()
}

/// Go's own export rule (no visibility keyword exists): exported iff the first Unicode letter of
/// `name` is uppercase — mirrors `unicode.IsUpper(rune(name[0]))` for every practical identifier
/// (ASCII or not). An empty name (never produced by the grammar for a real declaration) is `false`.
pub(crate) fn is_exported(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_exported_matches_go_capitalization_rule() {
        assert!(is_exported("Foo"));
        assert!(!is_exported("foo"));
        assert!(!is_exported(""));
        // Non-ASCII exported identifier (Go allows Unicode identifiers).
        assert!(is_exported("Ünïcode"));
    }

    // `end_line_of` has no standalone unit test here (it needs a real parsed node to exercise, and
    // this module has no parser access of its own) — it's exercised indirectly by every
    // `lang::loop_spans` test, which asserts exact `(start, end)` line pairs derived from it.
}

/// Adapts this crate's cursor to [`zzop_core::cst_depth::CstCursor`], which owns the walk.
///
/// 🔵 Three byte-identical copies of that walk used to live here, one per tree-sitter frontend,
/// each carrying a doc that said core could not own it because core must not depend on
/// `tree-sitter`. The dependency reason was right; the conclusion was not. A walk over a cursor
/// needs the cursor's three MOVES, not its type — so the walk moved and core stayed parser-free
/// (review ledger V132).
struct Cursor<'a, 'tree>(&'a mut tree_sitter::TreeCursor<'tree>);

impl zzop_core::cst_depth::CstCursor for Cursor<'_, '_> {
    fn goto_first_child(&mut self) -> bool {
        self.0.goto_first_child()
    }
    fn goto_next_sibling(&mut self) -> bool {
        self.0.goto_next_sibling()
    }
    fn goto_parent(&mut self) -> bool {
        self.0.goto_parent()
    }
}

/// Parse, then refuse a tree past the cap — the two steps that must never drift apart, so they live
/// in ONE call rather than a chain at the call site. The chain also has a cost this repo has a name
/// for: rustfmt wraps it, and that wrap alone pushed `parser-java-21/src/lib.rs` from 298 lines onto
/// the 300-line cap for reasons having nothing to do with that change (review ledger V113).
pub(crate) fn parse_within_depth(
    parser: &mut tree_sitter::Parser,
    text: &str,
) -> Option<tree_sitter::Tree> {
    let tree = parser.parse(text, None)?;
    // The cursor borrows the tree, so the borrow is scoped -- otherwise the tree cannot be moved
    // into the return value while a walk over it is still alive.
    let deep = {
        let mut walk = tree.walk();
        zzop_core::cst_depth::exceeds_max_depth(&mut Cursor(&mut walk))
    };
    (!deep).then_some(tree)
}

#[cfg(test)]
mod cst_depth_cap_tests;
