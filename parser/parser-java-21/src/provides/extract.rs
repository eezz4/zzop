//! The per-file `extract_http_provides` entry point — AST-grade reimplementation of the old lexical
//! `zzop_parser_java::provides::extract`'s span-overlap `enclosing_class` search: this crate walks the
//! real type-NESTING structure directly (a method's enclosing class is simply its AST parent's `body`
//! owner), so there is no separate "smallest containing span" search to port at all.

use tree_sitter::Node;
use zzop_core::{http_interface_key, IoProvide};

use super::annotations::{class_annotation_facts, method_route, route_path_state, RoutePathState};
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
/// ancestor's — module doc), then walks its direct members. A NON-LITERAL `@RequestMapping` prefix
/// (a constant reference) does NOT default to `""` — it BLOCKS this class's own direct routes (`blocked`),
/// mirroring the whole-corpus pass's `PrefixState::Unresolved` drop (`crate::project`): this per-file pass
/// has no cross-file visibility to resolve the constant, and keying the routes at the empty base would
/// fabricate phantoms under the wrong prefix. An ABSENT or empty `@RequestMapping` is still the legitimate
/// base `""`. (In the fused engine these per-file java http provides are replaced wholesale by the
/// whole-corpus pass — `run_java_provides_project_pass` — so this blocking is defense-in-depth plus honesty
/// for direct `extract_http_provides` callers; the whole-corpus pass is the one that reaches the join.)
fn walk_type(rel: &str, node: Node, src: &str, out: &mut Vec<IoProvide>) {
    let facts = class_annotation_facts(modifiers_of(node), src);
    let (prefix, blocked) = match facts.request_mapping_arg.as_deref() {
        None => (String::new(), false),
        Some(args) => match route_path_state(args) {
            RoutePathState::Literal(p) => (p, false),
            RoutePathState::Base => (String::new(), false),
            RoutePathState::NonLiteral(_) => (String::new(), true),
        },
    };
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    walk_body(rel, body, src, facts.is_controller, &prefix, blocked, out);
}

fn walk_body(
    rel: &str,
    body: Node,
    src: &str,
    is_controller: bool,
    prefix: &str,
    blocked: bool,
    out: &mut Vec<IoProvide>,
) {
    for child in valid_named_children(body) {
        if child.kind() == "enum_body_declarations" {
            for member in valid_named_children(child) {
                walk_member(rel, member, src, is_controller, prefix, blocked, out);
            }
        } else {
            walk_member(rel, child, src, is_controller, prefix, blocked, out);
        }
    }
}

fn walk_member(
    rel: &str,
    node: Node,
    src: &str,
    is_controller: bool,
    prefix: &str,
    blocked: bool,
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
    if blocked {
        // Class prefix is a non-literal this pass cannot resolve -> its own routes' paths are unknown. Skip
        // rather than key them under the empty base (a phantom). Nested types were already recursed above.
        return;
    }
    let routes = method_route(modifiers_of(node), src);
    if routes.is_empty() {
        return;
    }
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let symbol = node_text(name_node, src).to_string();
    let line = line_of(node);
    for (verb, path) in routes {
        let full_path = format!("{prefix}/{path}");
        out.push(IoProvide {
            body: None,
            kind: "http".to_string(),
            key: http_interface_key(&verb, &full_path),
            file: rel.to_string(),
            line,
            symbol: Some(symbol.clone()),
        });
    }
}
