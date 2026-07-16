//! Small helpers shared across `lang`/`provides`/`project` — mirrors `zzop_parser_go::util`'s role
//! ("how do we read a node's text / decide export-ness / compute a 1-based line" primitives defined
//! exactly once), adapted to `tree-sitter-java`'s field-vs-anonymous-keyword grammar shape (a Java
//! declaration's `modifiers` node is never itself a NAMED field — see this module's `modifiers_of` doc
//! — unlike Go's fielded declarations, so this crate needs a few helpers Go's `util` has no analogue
//! for).

use tree_sitter::Node;

/// 1-based START line of any tree-sitter node.
pub(crate) fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

/// 1-based END line of any tree-sitter node (the line its LAST byte sits on) — used for a body span's
/// closing brace line, mirroring the old lexical crate's `frame`-close `line` value.
pub(crate) fn end_line_of(node: Node) -> u32 {
    node.end_position().row as u32 + 1
}

/// The verbatim source text spanning `node`, empty on any UTF-8 boundary failure (never panics).
pub(crate) fn node_text<'a>(node: Node, src: &'a str) -> &'a str {
    node.utf8_text(src.as_bytes()).unwrap_or("")
}

/// `node`'s own NAMED children, skipping any that are themselves an error/missing subtree — the
/// shared "extract from the valid regions only" filter every walk in this crate applies before
/// matching on `Node::kind()`. Mirrors `zzop_parser_go::util::valid_named_children` exactly.
pub(crate) fn valid_named_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|c| !c.is_error() && !c.is_missing())
        .collect()
}

/// A declaration's `modifiers` node, when present — Java's grammar never wraps `modifiers` in a NAMED
/// field (`class_declaration: seq(optional($.modifiers), 'class', field('name', ...), ...)` — no
/// `field('modifiers', ...)` around it), unlike `name`/`body`/`type`/etc, so it must be found by kind
/// among `node`'s own named children rather than `child_by_field_name`. Always the FIRST such child
/// when present (grammar order places it before every other named child), but this scans all of them
/// defensively rather than assuming position.
pub(crate) fn modifiers_of(node: Node) -> Option<Node> {
    valid_named_children(node)
        .into_iter()
        .find(|c| c.kind() == "modifiers")
}

/// Whether `modifiers` (if present) carries the bare keyword token `kw` (`"public"`, `"static"`,
/// `"final"`, ...) as one of its own children. A keyword modifier is an ANONYMOUS token (not a named
/// node — `modifiers: repeat1(choice($._annotation, 'public', 'protected', ...))`), so this walks ALL
/// children (`node.children`, not `named_children`) rather than `valid_named_children`.
pub(crate) fn has_modifier_keyword(modifiers: Option<Node>, kw: &str) -> bool {
    let Some(modifiers) = modifiers else {
        return false;
    };
    let mut cursor = modifiers.walk();
    let found = modifiers
        .children(&mut cursor)
        .any(|c| !c.is_error() && !c.is_missing() && c.kind() == kw);
    found
}

/// Every `annotation`/`marker_annotation` directly attached via `modifiers` — the declaration's OWN
/// annotation set (never a superclass's, an interface default, or a meta-annotation's own composed
/// annotations — those are invisible to a per-declaration AST read, same documented limit the old
/// lexical crate's `provides.rs` module doc carries for its regex-based annotation-block scan).
pub(crate) fn annotations_of(modifiers: Option<Node<'_>>) -> Vec<Node<'_>> {
    let Some(modifiers) = modifiers else {
        return Vec::new();
    };
    valid_named_children(modifiers)
        .into_iter()
        .filter(|c| matches!(c.kind(), "annotation" | "marker_annotation"))
        .collect()
}

/// An `annotation`/`marker_annotation` node's own name, last-segment-only for a qualified spelling
/// (`@org.springframework.web.bind.annotation.GetMapping` -> `"GetMapping"`) — both node kinds carry a
/// `name` field of kind `identifier` or `scoped_identifier`.
pub(crate) fn annotation_name(node: Node, src: &str) -> Option<String> {
    let name = node.child_by_field_name("name")?;
    match name.kind() {
        "identifier" => Some(node_text(name, src).to_string()),
        "scoped_identifier" => {
            let last = name.child_by_field_name("name")?;
            Some(node_text(last, src).to_string())
        }
        _ => None,
    }
}

/// A full `annotation`'s raw argument text — the verbatim source BETWEEN the `annotation_argument_list`'s
/// own parens (comments/whitespace included, same "raw text" convention the old lexical crate's
/// `annotation_block`/`method_route` regex scan produced) — `None` for a `marker_annotation` (no parens
/// at all, the "annotation absent" signal `project::ClassRow::request_mapping_arg`'s own doc
/// distinguishes from `Some(String::new())` "present but empty").
pub(crate) fn annotation_raw_args(node: Node, src: &str) -> Option<String> {
    if node.kind() != "annotation" {
        return None;
    }
    let args = node.child_by_field_name("arguments")?;
    let raw = node_text(args, src);
    if raw.len() < 2 {
        return Some(String::new());
    }
    Some(raw[1..raw.len() - 1].to_string())
}

/// The simple (rightmost-segment) name of a `_type` node used in an `extends`/type-reference position —
/// `type_identifier` directly, the trailing `type_identifier` of a `scoped_type_identifier`
/// (`pkg.Base` -> `"Base"`), or a `generic_type`'s own base type recursively (`Base<T>` -> `"Base"`).
/// `None` for any other shape (never guessed).
pub(crate) fn simple_type_name(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "type_identifier" => Some(node_text(node, src).to_string()),
        "generic_type" => {
            let base = valid_named_children(node).into_iter().next()?;
            simple_type_name(base, src)
        }
        "scoped_type_identifier" => {
            let last = valid_named_children(node)
                .into_iter()
                .rfind(|c| c.kind() == "type_identifier")?;
            simple_type_name(last, src)
        }
        _ => None,
    }
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
    fn modifiers_of_finds_the_unfielded_modifiers_child() {
        let tree = crate::parse_tree("public class C {}").unwrap();
        let class = find_kind(tree.root_node(), "class_declaration").unwrap();
        assert!(modifiers_of(class).is_some());
        let tree2 = crate::parse_tree("class C {}").unwrap();
        let class2 = find_kind(tree2.root_node(), "class_declaration").unwrap();
        assert!(modifiers_of(class2).is_none());
    }

    #[test]
    fn has_modifier_keyword_checks_anonymous_tokens() {
        let tree = crate::parse_tree("public static final class C {}").unwrap();
        let class = find_kind(tree.root_node(), "class_declaration").unwrap();
        let modifiers = modifiers_of(class);
        assert!(has_modifier_keyword(modifiers, "public"));
        assert!(has_modifier_keyword(modifiers, "static"));
        assert!(has_modifier_keyword(modifiers, "final"));
        assert!(!has_modifier_keyword(modifiers, "private"));
        assert!(!has_modifier_keyword(None, "public"));
    }

    #[test]
    fn annotations_of_and_annotation_name_read_marker_and_full_annotations() {
        let src = "@Deprecated\n@RequestMapping(\"/x\")\nclass C {}";
        let tree = crate::parse_tree(src).unwrap();
        let class = find_kind(tree.root_node(), "class_declaration").unwrap();
        let anns = annotations_of(modifiers_of(class));
        assert_eq!(anns.len(), 2);
        let names: Vec<String> = anns
            .iter()
            .map(|a| annotation_name(*a, src).unwrap())
            .collect();
        assert_eq!(names, vec!["Deprecated", "RequestMapping"]);
    }

    #[test]
    fn annotation_raw_args_distinguishes_marker_from_empty_parens() {
        let src = "@Deprecated\n@RequestMapping()\nclass C {}";
        let tree = crate::parse_tree(src).unwrap();
        let class = find_kind(tree.root_node(), "class_declaration").unwrap();
        let anns = annotations_of(modifiers_of(class));
        assert_eq!(annotation_raw_args(anns[0], src), None);
        assert_eq!(annotation_raw_args(anns[1], src), Some(String::new()));
    }

    #[test]
    fn simple_type_name_resolves_plain_scoped_and_generic_types() {
        let src = "class C extends pkg.Base<T> {}";
        let tree = crate::parse_tree(src).unwrap();
        let superclass = find_kind(tree.root_node(), "superclass").unwrap();
        let ty = valid_named_children(superclass).into_iter().next().unwrap();
        assert_eq!(simple_type_name(ty, src), Some("Base".to_string()));
    }
}
