//! Small helpers shared across `lang`/`adapters` — mirrors `zzop_parser_go::util`'s role ("how do we
//! read a node's text / decide export-ness / compute a 1-based line" primitives defined exactly once)
//! and `zzop_parser_java_21::util`'s field-vs-anonymous-token handling, adapted to
//! `tree-sitter-c-sharp`'s OWN grammar shape:
//!
//! - A C# declaration's repeated keyword modifiers (`public`, `static`, `const`, ...) are each their
//!   OWN NAMED node of kind `modifier` (unlike Java, where the keyword itself is an anonymous token
//!   inside one unfielded `modifiers` wrapper) — so `modifiers_of`/`has_modifier` read `modifier`
//!   nodes by kind among `node`'s own named children (never a field; the grammar attaches zero or more
//!   `modifier` children directly), and the modifier keyword text comes from the `modifier` node's OWN
//!   span (it wraps exactly one anonymous token, no further structure).
//! - An attribute (`[ApiController]`, `[Route("x")]`) is likewise never wrapped in a single fielded
//!   "attributes" slot: zero or more `attribute_list` nodes attach directly as unfielded children of the
//!   declaration, each itself holding one or more `attribute` children (`[A, B]` is ONE `attribute_list`
//!   with two `attribute`s; `[A][B]` is two separate `attribute_list`s with one each) —
//!   `attributes_of` flattens every `attribute_list` reachable this way into one `Vec<Node>` of
//!   `attribute` nodes, mirroring `zzop_parser_java_21::util::annotations_of`'s "this declaration's own
//!   annotation set only" scope (never a superclass's or a meta-attribute's own composed attributes).
//! - `global`/`static` on a `using_directive` are anonymous tokens too (no field), so
//!   `has_anonymous_child` walks ALL of a node's children (not just named ones) the same way
//!   `zzop_parser_java_21::util::has_modifier_keyword` does for Java's bare keyword tokens.

use tree_sitter::Node;

/// 1-based START line of any tree-sitter node.
pub(crate) fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

/// 1-based END line of any tree-sitter node (the line its LAST byte sits on) — used for a body span's
/// closing brace line, mirroring `zzop_parser_java_21::util::end_line_of` exactly.
pub(crate) fn end_line_of(node: Node) -> u32 {
    node.end_position().row as u32 + 1
}

/// The verbatim source text spanning `node`, empty on any UTF-8 boundary failure (never panics).
pub(crate) fn node_text<'a>(node: Node, src: &'a str) -> &'a str {
    node.utf8_text(src.as_bytes()).unwrap_or("")
}

/// `node`'s own NAMED children, skipping any that are themselves an error/missing subtree — the shared
/// "extract from the valid regions only" filter every walk in this crate applies before matching on
/// `Node::kind()`. Mirrors `zzop_parser_go`/`zzop_parser_java_21::util::valid_named_children` exactly.
pub(crate) fn valid_named_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|c| !c.is_error() && !c.is_missing())
        .collect()
}

/// Every `modifier` node directly attached to `node` (module doc: never fielded, never wrapped) — the
/// declaration's own written keyword modifiers, in source order.
pub(crate) fn modifiers_of(node: Node) -> Vec<Node> {
    valid_named_children(node)
        .into_iter()
        .filter(|c| c.kind() == "modifier")
        .collect()
}

/// Whether `mods` (as returned by [`modifiers_of`]) contains the bare keyword `kw` (`"public"`,
/// `"static"`, `"const"`, `"readonly"`, ...) as one of its own node's verbatim text.
pub(crate) fn has_modifier(mods: &[Node], kw: &str, src: &str) -> bool {
    mods.iter().any(|m| node_text(*m, src) == kw)
}

/// Whether `node` has an ANONYMOUS child token of exactly kind `kw` — module doc's `global`/`static`
/// `using_directive` prefix check (neither is a field, unlike a `modifier` node's own named-child
/// status for a declaration). Walks ALL children (`node.children`, not `named_children`), mirroring
/// `zzop_parser_java_21::util::has_modifier_keyword`'s identical "anonymous token" walk.
pub(crate) fn has_anonymous_child(node: Node, kw: &str) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|c| !c.is_error() && !c.is_missing() && c.kind() == kw);
    found
}

/// Every `attribute` node reachable from `node`'s own directly-attached `attribute_list` children
/// (module doc) — the declaration's OWN attribute set (never an ancestor's, never a composed/meta
/// attribute's own nested attributes — those are invisible to a per-declaration AST read, the same
/// documented limit `zzop_parser_java_21::util::annotations_of` carries for Java annotations).
pub(crate) fn attributes_of(node: Node) -> Vec<Node> {
    let mut out = Vec::new();
    for list in valid_named_children(node) {
        if list.kind() != "attribute_list" {
            continue;
        }
        for attr in valid_named_children(list) {
            if attr.kind() == "attribute" {
                out.push(attr);
            }
        }
    }
    out
}

/// An `attribute` node's own name, last-segment-only for a qualified spelling
/// (`@System.Web.Http.Route` style dotted attribute names are rare in practice but handled the same way
/// `zzop_parser_java_21::util::annotation_name` handles a qualified Java annotation) — the `name` field
/// is `identifier` (simple), `generic_name` (an attribute is never actually generic in valid C#, but the
/// grammar allows the node shape; its own leading `identifier` is used), `qualified_name` (rightmost
/// `name` field, recursively — the qualifier itself is never inspected), or `alias_qualified_name`
/// (its own `name` field, which is a `_simple_name`: `identifier` or `generic_name`).
pub(crate) fn attribute_name(node: Node, src: &str) -> Option<String> {
    let name = node.child_by_field_name("name")?;
    simple_name_text(name, src)
}

fn simple_name_text(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(node_text(node, src).to_string()),
        "generic_name" => valid_named_children(node)
            .into_iter()
            .next()
            .map(|n| node_text(n, src).to_string()),
        "qualified_name" | "alias_qualified_name" => {
            let last = node.child_by_field_name("name")?;
            simple_name_text(last, src)
        }
        _ => None,
    }
}

/// An `attribute`'s raw argument text — the verbatim source BETWEEN its `attribute_argument_list`'s own
/// parens (comments/whitespace included) — `None` when the attribute carries no argument list at all
/// (bare `[ApiController]`, the "attribute absent-vs-present-but-empty" distinction
/// `zzop_parser_java_21::util::annotation_raw_args`'s doc pins for Java's marker annotations).
pub(crate) fn attribute_raw_args(node: Node, src: &str) -> Option<String> {
    if node.kind() != "attribute" {
        return None;
    }
    let args = valid_named_children(node)
        .into_iter()
        .find(|c| c.kind() == "attribute_argument_list")?;
    let raw = node_text(args, src);
    if raw.len() < 2 {
        return Some(String::new());
    }
    Some(raw[1..raw.len() - 1].to_string())
}

/// The decoded-ish text of a plain `string_literal` node: the node's own span includes its `"..."`
/// delimiters (exactly one byte each), so stripping the first/last byte yields the interior text —
/// mirrors `zzop_parser_go::util::string_literal_text`'s "verbatim, no escape decoding" convention
/// exactly (this crate's own HTTP-literal reads never plausibly carry an escape sequence). `None` for
/// any other node kind (an interpolated/verbatim/raw string literal is a DIFFERENT grammar node kind —
/// never guessed at here).
pub(crate) fn string_literal_text(node: Node, src: &str) -> Option<String> {
    if node.kind() != "string_literal" {
        return None;
    }
    let raw = node_text(node, src);
    if raw.len() < 2 {
        return Some(String::new());
    }
    Some(raw[1..raw.len() - 1].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_kind(child, kind) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn modifiers_of_and_has_modifier_read_named_modifier_nodes() {
        let tree = crate::parse_tree("public static class C {}").unwrap();
        let class = find_kind(tree.root_node(), "class_declaration").unwrap();
        let mods = modifiers_of(class);
        assert_eq!(mods.len(), 2);
        let src = "public static class C {}";
        assert!(has_modifier(&mods, "public", src));
        assert!(has_modifier(&mods, "static", src));
        assert!(!has_modifier(&mods, "private", src));
    }

    #[test]
    fn attributes_of_and_attribute_name_read_marker_and_argument_attributes() {
        let src = "[ApiController]\n[Route(\"/x\")]\nclass C {}";
        let tree = crate::parse_tree(src).unwrap();
        let class = find_kind(tree.root_node(), "class_declaration").unwrap();
        let attrs = attributes_of(class);
        assert_eq!(attrs.len(), 2);
        let names: Vec<String> = attrs
            .iter()
            .map(|a| attribute_name(*a, src).unwrap())
            .collect();
        assert_eq!(names, vec!["ApiController", "Route"]);
        assert_eq!(attribute_raw_args(attrs[0], src), None);
        assert_eq!(
            attribute_raw_args(attrs[1], src),
            Some("\"/x\"".to_string())
        );
    }

    #[test]
    fn string_literal_text_strips_delimiters_and_rejects_other_kinds() {
        let src = "class C { string a = \"hi\"; string b = \"\"; }";
        let tree = crate::parse_tree(src).unwrap();
        let lit = find_kind(tree.root_node(), "string_literal").unwrap();
        assert_eq!(string_literal_text(lit, src), Some("hi".to_string()));
    }
}
