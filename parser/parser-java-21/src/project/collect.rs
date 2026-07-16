//! Per-file `ClassRow` collection — walks every `class`/`interface`/`enum`/`record`/`@interface`
//! declaration in a file (top-level + nested, keyed by SIMPLE name, module doc's "known limits"),
//! computing its `extends` target, class-gating facts, own `static final String` constants, and own
//! resolved method routes in one AST pass.

use std::collections::HashMap;
use tree_sitter::Node;

use super::{ClassRow, MethodRoute};
use crate::lang::symbols::is_type_decl_kind;
use crate::provides::{class_annotation_facts, method_route};
use crate::util::{line_of, modifiers_of, node_text, simple_type_name, valid_named_children};

/// Walks `root`'s top-level children, recording one `ClassRow` PER type declaration found (top-level and
/// nested alike) into `rows_by_name`, keyed by simple name — ambiguity (2+ declarations sharing a name)
/// is resolved later by the caller (`walk::extract_http_provides_project`), not here.
pub(super) fn collect_from_root(
    rel: &str,
    root: Node,
    src: &str,
    rows_by_name: &mut HashMap<String, Vec<ClassRow>>,
) {
    for child in valid_named_children(root) {
        if is_type_decl_kind(child.kind()) {
            walk_class(rel, child, src, rows_by_name);
        }
    }
}

fn walk_class(rel: &str, node: Node, src: &str, rows_by_name: &mut HashMap<String, Vec<ClassRow>>) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(name_node, src).to_string();
    let modifiers = modifiers_of(node);
    let extends = extends_of(node, src);
    let facts = class_annotation_facts(modifiers, src);
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let always_const = matches!(
        node.kind(),
        "interface_declaration" | "annotation_type_declaration"
    );

    let mut constants = HashMap::new();
    let mut methods = Vec::new();
    walk_body(
        rel,
        body,
        src,
        always_const,
        &mut constants,
        &mut methods,
        rows_by_name,
    );

    rows_by_name.entry(name).or_default().push(ClassRow {
        file: rel.to_string(),
        extends,
        is_controller: facts.is_controller,
        request_mapping_arg: facts.request_mapping_arg,
        constants,
        methods,
    });
}

/// A `class_declaration`'s own `superclass` target, simple-name-resolved — `None` for every other
/// declaration kind (only classes can `extends` a class at all).
fn extends_of(node: Node, src: &str) -> Option<String> {
    if node.kind() != "class_declaration" {
        return None;
    }
    let superclass = node.child_by_field_name("superclass")?;
    let ty = valid_named_children(superclass).into_iter().next()?;
    simple_type_name(ty, src)
}

#[allow(clippy::too_many_arguments)]
fn walk_body(
    rel: &str,
    body: Node,
    src: &str,
    always_const: bool,
    constants: &mut HashMap<String, String>,
    methods: &mut Vec<MethodRoute>,
    rows_by_name: &mut HashMap<String, Vec<ClassRow>>,
) {
    for child in valid_named_children(body) {
        if child.kind() == "enum_body_declarations" {
            for member in valid_named_children(child) {
                walk_member(
                    rel,
                    member,
                    src,
                    always_const,
                    constants,
                    methods,
                    rows_by_name,
                );
            }
        } else {
            walk_member(
                rel,
                child,
                src,
                always_const,
                constants,
                methods,
                rows_by_name,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_member(
    rel: &str,
    node: Node,
    src: &str,
    always_const: bool,
    constants: &mut HashMap<String, String>,
    methods: &mut Vec<MethodRoute>,
    rows_by_name: &mut HashMap<String, Vec<ClassRow>>,
) {
    match node.kind() {
        k if is_type_decl_kind(k) => walk_class(rel, node, src, rows_by_name), // own independent row.
        "field_declaration" => collect_constant(node, src, false, constants),
        "constant_declaration" => collect_constant(node, src, always_const, constants),
        "method_declaration" | "constructor_declaration" => {
            if let Some((verb, path)) = method_route(modifiers_of(node), src) {
                if let Some(name_node) = node.child_by_field_name("name") {
                    methods.push(MethodRoute {
                        line: line_of(node),
                        name: node_text(name_node, src).to_string(),
                        verb,
                        path,
                    });
                }
            }
        }
        _ => {}
    }
}

/// This declaration's own `String`-typed, SCREAMING_SNAKE_CASE-named constant(s) — `always_const`
/// bypasses the `static`+`final` modifier check (an interface/annotation-type `constant_declaration` is
/// implicitly `public static final` with no keywords required, JLS 9.3/9.6.1). Mirrors the old lexical
/// crate's `constant_decl_re`/`interface_field_re`'s exact type/name-shape restriction.
fn collect_constant(
    node: Node,
    src: &str,
    always_const: bool,
    constants: &mut HashMap<String, String>,
) {
    let modifiers = modifiers_of(node);
    let is_static_final = crate::util::has_modifier_keyword(modifiers, "static")
        && crate::util::has_modifier_keyword(modifiers, "final");
    if !always_const && !is_static_final {
        return;
    }
    let Some(ty) = node.child_by_field_name("type") else {
        return;
    };
    if node_text(ty, src) != "String" {
        return;
    }
    let mut cursor = node.walk();
    for declarator in node.children_by_field_name("declarator", &mut cursor) {
        if declarator.is_error() || declarator.is_missing() {
            continue;
        }
        let Some(name_node) = declarator.child_by_field_name("name") else {
            continue;
        };
        if name_node.kind() != "identifier" {
            continue;
        }
        let name = node_text(name_node, src);
        if !is_screaming_snake_case(name) {
            continue;
        }
        let Some(value_node) = declarator.child_by_field_name("value") else {
            continue; // no initializer — nothing to record.
        };
        constants.insert(name.to_string(), node_text(value_node, src).to_string());
    }
}

/// `[A-Z][A-Z0-9_]*` — the Java constant-naming convention the old lexical crate's regexes required.
fn is_screaming_snake_case(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}
