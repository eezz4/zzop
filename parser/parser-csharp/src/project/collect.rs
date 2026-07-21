//! Per-file `ClassRow` collection — walks every `class_declaration` (top-level, namespace-nested, and
//! type-nested alike, each an INDEPENDENT row keyed by simple name), recording its controller-gating facts,
//! class-`[Route]` prefix state, own `const string`/`static readonly string` constants, and own resolved
//! method-route tri-states in one AST pass. Mirrors `zzop_parser_java_21::project::collect`, adapted to
//! `tree-sitter-c-sharp`'s grammar (namespace-transparent scope, `class_declaration`-only controllers).

use std::collections::HashMap;
use tree_sitter::Node;

use super::{ClassPrefix, ClassRow, MethodPath, MethodRoute};
use crate::adapters::provides::attribute_controller::{
    attr_path_state, method_route_state, substitute_controller_token, PathState,
};
use crate::util::{
    attribute_name, attribute_raw_args, attributes_of, has_modifier, line_of, modifiers_of,
    node_text, string_literal_text, valid_named_children,
};

/// Recurses THROUGH block namespaces transparently (like `lang::symbols`), recording one `ClassRow` per
/// `class_declaration` found (top-level and nested alike) into `rows_by_name`, keyed by simple name.
pub(super) fn collect_from_root(
    rel: &str,
    root: Node,
    src: &str,
    rows_by_name: &mut HashMap<String, Vec<ClassRow>>,
) {
    walk_scope(rel, root, src, rows_by_name);
}

fn walk_scope(rel: &str, node: Node, src: &str, rows_by_name: &mut HashMap<String, Vec<ClassRow>>) {
    for child in valid_named_children(node) {
        match child.kind() {
            "namespace_declaration" => {
                if let Some(body) = child.child_by_field_name("body") {
                    walk_scope(rel, body, src, rows_by_name);
                }
            }
            "class_declaration" => walk_class(rel, child, src, rows_by_name),
            _ => {}
        }
    }
}

fn walk_class(rel: &str, node: Node, src: &str, rows_by_name: &mut HashMap<String, Vec<ClassRow>>) {
    let simple_name = node
        .child_by_field_name("name")
        .map(|n| node_text(n, src))
        .unwrap_or("")
        .to_string();
    let attrs = attributes_of(node);
    let is_controller = attrs.iter().any(|a| {
        matches!(
            attribute_name(*a, src).as_deref(),
            Some("ApiController") | Some("Controller")
        )
    }) || simple_name.ends_with("Controller");
    // `partial` modifier -> this row may be one half of a class split across files (`super::merge_partial_rows`).
    let is_partial = has_modifier(&modifiers_of(node), "partial", src);
    let prefix = class_prefix_state(&attrs, src, &simple_name);

    let mut constants = HashMap::new();
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for member in valid_named_children(body) {
            match member.kind() {
                // A nested class is its OWN independent row (nested controllers gate independently, and a
                // nested class's own constants must NOT leak into this class's scan — precision like Java's).
                "class_declaration" => walk_class(rel, member, src, rows_by_name),
                "field_declaration" => collect_constant(member, src, &mut constants),
                // Route methods are collected regardless of THIS row's own controller-ness: a `partial class`
                // half may carry a route method while the `[ApiController]` attribute lives on another half
                // (`is_controller = any` after merge). Emission (`super::emit_controller_routes`) is what gates
                // on the resolved controller-ness — a non-controller row's methods are collected but ignored.
                "method_declaration" | "constructor_declaration" => {
                    collect_method(rel, member, src, &mut methods);
                }
                _ => {}
            }
        }
    }

    rows_by_name
        .entry(simple_name.clone())
        .or_default()
        .push(ClassRow {
            file: rel.to_string(),
            simple_name,
            is_controller,
            is_partial,
            prefix,
            constants,
            methods,
        });
}

/// The class-level `[Route]` prefix state — mirrors the per-file `attribute_controller::class_route_prefix`
/// exactly (a literal wins with `[controller]` substitution; an absent `[Route]`, or `[Route()]`, is the
/// empty prefix), but carries a NON-LITERAL prefix's raw args forward for corpus resolution instead of blocking.
fn class_prefix_state(attrs: &[Node], src: &str, simple_name: &str) -> ClassPrefix {
    let Some(route_attr) = attrs
        .iter()
        .find(|a| attribute_name(**a, src).as_deref() == Some("Route"))
    else {
        return ClassPrefix::Literal(String::new());
    };
    let args = attribute_raw_args(*route_attr, src).unwrap_or_default();
    match attr_path_state(&args) {
        PathState::Literal(raw) => {
            ClassPrefix::Literal(substitute_controller_token(&raw, simple_name))
        }
        PathState::Absent => ClassPrefix::Literal(String::new()),
        PathState::NonLiteral(a) => ClassPrefix::NonLiteral(a),
    }
}

/// Records one route method's `(verb, path-state)` (`attribute_controller::method_route_state`) as a
/// `MethodRoute`, carrying a non-literal path's raw args forward for corpus resolution — a non-route method
/// (no recognized verb attribute) yields nothing.
fn collect_method(rel: &str, node: Node, src: &str, methods: &mut Vec<MethodRoute>) {
    let Some((verb, state)) = method_route_state(&attributes_of(node), src) else {
        return;
    };
    let path = match state {
        PathState::Literal(p) => MethodPath::Literal(p),
        PathState::Absent => MethodPath::Literal(String::new()),
        PathState::NonLiteral(a) => MethodPath::NonLiteral(a),
    };
    methods.push(MethodRoute {
        file: rel.to_string(),
        line: line_of(node),
        symbol: node
            .child_by_field_name("name")
            .map(|n| node_text(n, src).to_string()),
        verb,
        path,
    });
}

/// This declaration's own `const string`/`static readonly string` fields whose initializer is a SIMPLE
/// string literal — the `const`+type gate mirrors `lang::symbols::member::emit_fields`. A concatenated or
/// computed initializer (`"/a" + "/b"`, a member reference) is deliberately NOT recorded (v1 scope): the
/// referencing route then stays unresolved and is counted, never guessed.
fn collect_constant(node: Node, src: &str, constants: &mut HashMap<String, String>) {
    let mods = modifiers_of(node);
    let is_const = has_modifier(&mods, "const", src);
    let is_static_readonly =
        has_modifier(&mods, "static", src) && has_modifier(&mods, "readonly", src);
    if !is_const && !is_static_readonly {
        return;
    }
    let Some(declaration) = valid_named_children(node)
        .into_iter()
        .find(|c| c.kind() == "variable_declaration")
    else {
        return;
    };
    let Some(ty) = declaration.child_by_field_name("type") else {
        return;
    };
    if !is_string_type(node_text(ty, src)) {
        return;
    }
    for declarator in valid_named_children(declaration) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = declarator.child_by_field_name("name") else {
            continue;
        };
        let Some(value) = declarator_value(declarator, name_node) else {
            continue; // no initializer, or a bracketed-argument-list only — nothing to record.
        };
        // Only a simple `string_literal` value is recorded; `string_literal_text` returns `None` for any
        // other value kind (a `binary_expression` concatenation, a `member_access_expression`, ...).
        if let Some(lit) = string_literal_text(value, src) {
            constants.insert(node_text(name_node, src).to_string(), lit);
        }
    }
}

/// The initializer expression of a `variable_declarator`: the first named child that is neither the `name`
/// identifier nor a `bracketed_argument_list` (the C# grammar attaches the initializer as a direct sibling
/// of the name, not a field).
fn declarator_value<'a>(declarator: Node<'a>, name_node: Node) -> Option<Node<'a>> {
    valid_named_children(declarator)
        .into_iter()
        .find(|c| c.id() != name_node.id() && c.kind() != "bracketed_argument_list")
}

/// True for the C# string type in any of its spellings a route constant plausibly uses: the `string`
/// keyword (`predefined_type`), the `String` BCL alias, or a qualified `System.String` (last segment only).
fn is_string_type(ty: &str) -> bool {
    let last = ty.rsplit('.').next().unwrap_or(ty).trim();
    last == "string" || last == "String"
}
