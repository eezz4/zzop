//! The per-file `extract_http_provides` entry point — AST-grade reimplementation of the old lexical
//! `zzop_parser_java::provides::extract`'s span-overlap `enclosing_class` search: this crate walks the
//! real type-NESTING structure directly (a method's enclosing class is simply its AST parent's `body`
//! owner), so there is no separate "smallest containing span" search to port at all.

use tree_sitter::Node;
use zzop_core::{http_interface_key, IoProvide};

use super::annotations::{class_annotation_facts, first_quoted_string, method_route};
use crate::lang::symbols::is_type_decl_kind;
use crate::util::{line_of, modifiers_of, node_text, valid_named_children};

/// Extracts Spring MVC HTTP route `IoProvide`s from one Java file's raw source — see the parent module
/// doc (`provides/mod.rs`) for the exact annotation shapes recognized and the class/method gating rule.
/// Never panics: empty on parse failure.
pub fn extract_http_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for child in valid_named_children(tree.root_node()) {
        if is_type_decl_kind(child.kind()) {
            walk_type(rel, child, text, &mut out);
        }
    }
    out
}

/// One class/interface/enum/record/annotation-type declaration: reads its OWN gating facts (never an
/// ancestor's — module doc), then walks its direct members. A non-literal `@RequestMapping` prefix
/// defaults to `""`, same as an absent one — this per-file pass has no cross-file visibility to resolve
/// a constant reference (see `crate::project` for the whole-corpus pass that does).
fn walk_type(rel: &str, node: Node, src: &str, out: &mut Vec<IoProvide>) {
    let facts = class_annotation_facts(modifiers_of(node), src);
    let prefix = facts
        .request_mapping_arg
        .as_deref()
        .and_then(first_quoted_string)
        .unwrap_or_default();
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    walk_body(rel, body, src, facts.is_controller, &prefix, out);
}

fn walk_body(
    rel: &str,
    body: Node,
    src: &str,
    is_controller: bool,
    prefix: &str,
    out: &mut Vec<IoProvide>,
) {
    for child in valid_named_children(body) {
        if child.kind() == "enum_body_declarations" {
            for member in valid_named_children(child) {
                walk_member(rel, member, src, is_controller, prefix, out);
            }
        } else {
            walk_member(rel, child, src, is_controller, prefix, out);
        }
    }
}

fn walk_member(
    rel: &str,
    node: Node,
    src: &str,
    is_controller: bool,
    prefix: &str,
    out: &mut Vec<IoProvide>,
) {
    if is_type_decl_kind(node.kind()) {
        walk_type(rel, node, src, out); // a nested type gates independently — its own annotations only.
        return;
    }
    if !is_controller
        || !matches!(
            node.kind(),
            "method_declaration" | "constructor_declaration"
        )
    {
        return;
    }
    let Some((verb, path)) = method_route(modifiers_of(node), src) else {
        return;
    };
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let full_path = format!("{prefix}/{path}");
    out.push(IoProvide {
        body: None,
        kind: "http".to_string(),
        key: http_interface_key(&verb, &full_path),
        file: rel.to_string(),
        line: line_of(node),
        symbol: Some(node_text(name_node, src).to_string()),
    });
}
