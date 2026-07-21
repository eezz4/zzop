//! One type's own member declarations (methods, constructors, properties, `const`/`static readonly`
//! fields) -> `SourceSymbol` — see the parent module doc (`mod.rs`) for the qualified-naming and
//! kind-mapping rules this applies.

use tree_sitter::Node;
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::util::{
    end_line_of, has_modifier, line_of, modifiers_of, node_text, valid_named_children,
};

/// Dispatches one `declaration_list` member node by kind — see parent module doc for the recognized
/// shapes; anything else (an indexer, operator, event, destructor, static constructor, a nested type —
/// handled one level up by `super::emit_body` before this fn is ever called for it) contributes no
/// symbol.
pub(super) fn emit_member(
    rel: &str,
    node: Node,
    src: &str,
    path: &[String],
    out: &mut Vec<SourceSymbol>,
) {
    match node.kind() {
        "method_declaration" => emit_method(rel, node, src, path, out),
        "constructor_declaration" => emit_ctor(rel, node, src, path, out),
        "property_declaration" => emit_property(rel, node, src, path, out),
        "field_declaration" => emit_fields(rel, node, src, path, out),
        _ => {}
    }
}

fn emit_method(rel: &str, node: Node, src: &str, path: &[String], out: &mut Vec<SourceSymbol>) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let method_name = node_text(name_node, src);
    let (body_start, body_end) = node
        .child_by_field_name("body")
        .map(|b| (Some(line_of(b)), Some(end_line_of(b))))
        .unwrap_or((None, None));
    push(
        rel,
        path,
        method_name,
        SourceSymbolKind::Function,
        line_of(node),
        has_modifier(&modifiers_of(node), "public", src),
        body_start,
        body_end,
        out,
    );
}

fn emit_ctor(rel: &str, node: Node, src: &str, path: &[String], out: &mut Vec<SourceSymbol>) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let ctor_name = node_text(name_node, src);
    let (body_start, body_end) = node
        .child_by_field_name("body")
        .map(|b| (Some(line_of(b)), Some(end_line_of(b))))
        .unwrap_or((None, None));
    push(
        rel,
        path,
        ctor_name,
        SourceSymbolKind::Function,
        line_of(node),
        has_modifier(&modifiers_of(node), "public", src),
        body_start,
        body_end,
        out,
    );
}

/// `property_declaration` -> `Const` unconditionally (module doc). Body span comes from the
/// `accessors` field (`accessor_list`, `{ get; set; }`) when present; an expression-bodied property
/// (`int X => 5;`, `value` field instead) carries `None`/`None` — kept simple, out of v1 span scope.
fn emit_property(rel: &str, node: Node, src: &str, path: &[String], out: &mut Vec<SourceSymbol>) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let (body_start, body_end) = node
        .child_by_field_name("accessors")
        .map(|a| (Some(line_of(a)), Some(end_line_of(a))))
        .unwrap_or((None, None));
    push(
        rel,
        path,
        node_text(name_node, src),
        SourceSymbolKind::Const,
        line_of(node),
        has_modifier(&modifiers_of(node), "public", src),
        body_start,
        body_end,
        out,
    );
}

/// `field_declaration` -> `Const` only when `const` is present, or BOTH `static` and `readonly` are —
/// module doc's field gate. One symbol per comma-separated declarator name, all sharing the
/// declaration's own line.
fn emit_fields(rel: &str, node: Node, src: &str, path: &[String], out: &mut Vec<SourceSymbol>) {
    let mods = modifiers_of(node);
    let is_const = has_modifier(&mods, "const", src);
    let is_static_readonly =
        has_modifier(&mods, "static", src) && has_modifier(&mods, "readonly", src);
    if !is_const && !is_static_readonly {
        return; // an instance/plain-static field — never symbol-surface, module doc.
    }
    let exported = has_modifier(&mods, "public", src);
    let line = line_of(node);
    let Some(declaration) = valid_named_children(node)
        .into_iter()
        .find(|c| c.kind() == "variable_declaration")
    else {
        return;
    };
    for declarator in valid_named_children(declaration) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = declarator.child_by_field_name("name") else {
            continue;
        };
        push(
            rel,
            path,
            node_text(name_node, src),
            SourceSymbolKind::Const,
            line,
            exported,
            None,
            None,
            out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push(
    rel: &str,
    path: &[String],
    own_name: &str,
    kind: SourceSymbolKind,
    line: u32,
    exported: bool,
    body_start: Option<u32>,
    body_end: Option<u32>,
    out: &mut Vec<SourceSymbol>,
) {
    let name = format!("{}.{own_name}", path.join("."));
    out.push(SourceSymbol {
        id: format!("{rel}#{name}"),
        file: rel.to_string(),
        name,
        kind,
        line,
        exported,
        is_default: false,
        body_start,
        body_end,
        write_sites: Vec::new(),
    });
}
