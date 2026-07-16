//! Small helpers shared across `lang` and `adapters` — kept in one place so the "how do we read a
//! node's text / decide export-ness / compute a 1-based line" primitives are defined exactly once,
//! mirroring how `zzop_parser_rust`'s crate root centralizes `line_of` for the same reason.

use tree_sitter::Node;

/// 1-based line of any tree-sitter node — `node.start_position().row` is 0-based; the task-level
/// contract (and every other parser in this workspace) reports 1-based lines.
pub(crate) fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

/// The verbatim source text spanning `node`, empty on any UTF-8 boundary failure (never panics —
/// tree-sitter guarantees valid byte ranges for a well-formed tree, but a defensive empty string
/// keeps this infallible for callers).
pub(crate) fn node_text<'a>(node: Node, src: &'a str) -> &'a str {
    node.utf8_text(src.as_bytes()).unwrap_or("")
}

/// The decoded-ish text of a Go string literal node (`interpreted_string_literal` or
/// `raw_string_literal`): the node's own span includes its delimiter (`"..."` or `` `...` ``, both
/// exactly one byte each), so stripping the first/last byte yields the interior text. Deliberately
/// NOT full escape-sequence decoding (`\n` stays the two literal characters `\` `n`) — this crate
/// only ever reads import paths and HTTP literals through this helper, neither of which plausibly
/// carries an escape sequence in practice; full decoding would mean walking the node's
/// `escape_sequence`/`*_content` children and is not worth the complexity for a v1 "verbatim string"
/// contract. `None` for any other node kind (never guessed).
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
}
