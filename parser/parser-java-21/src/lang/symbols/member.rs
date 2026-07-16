//! One type's own member declarations (methods, constructors, `static final` fields, and nested types)
//! -> `SourceSymbol` — see the parent module doc (`mod.rs`) for the qualified-naming and kind-mapping
//! rules this applies.
//!
//! ## Parity deviation vs the retired lexical crate (deliberate — opus review F1)
//! A BODYLESS method (interface/abstract method) emits a symbol here with `body_start`/`body_end` =
//! `None`; the old lexical `parser-java` emitted NO symbol for these at all (its brace-matcher only
//! saw method BODIES, and it pinned that behavior by test). The CST view is the honest one — the
//! declaration exists and is API surface — but two observable outputs shift for Java trees containing
//! interfaces/abstract methods: the symbol SET gains those members, and the exported-symbol count in
//! the coverage/diagnostics census grows accordingly (interface members are implicitly public). Same
//! sanctioned tier-upgrade class as the other scope expansions (enum/record/annotation types,
//! `static final` consts), called out separately because it REVERSES a behavior the old crate pinned
//! rather than merely adding a shape the old crate never saw.

use tree_sitter::Node;
use zzop_core::{SourceSymbol, SourceSymbolKind};

use super::symbol_exported;
use crate::util::{end_line_of, has_modifier_keyword, line_of, modifiers_of, node_text};

/// Dispatches one `class_body`/`interface_body`/`annotation_type_body`/`enum_body_declarations` member
/// node by kind — see parent module doc for the recognized shapes; anything else (a `block`,
/// `static_initializer`, an annotation-type element, `;`) contributes no symbol.
pub(super) fn emit_member(
    rel: &str,
    node: Node,
    src: &str,
    path: &[String],
    implicit_public: bool,
    out: &mut Vec<SourceSymbol>,
) {
    match node.kind() {
        k if super::is_type_decl_kind(k) => {
            super::emit_type(rel, node, src, path, implicit_public, out)
        }
        "method_declaration" => emit_method(rel, node, src, path, implicit_public, out),
        "constructor_declaration" => emit_ctor(rel, node, src, path, implicit_public, out),
        "compact_constructor_declaration" => {
            emit_compact_ctor(rel, node, src, path, implicit_public, out)
        }
        "field_declaration" => emit_fields(rel, node, src, path, implicit_public, false, out),
        "constant_declaration" => emit_fields(rel, node, src, path, implicit_public, true, out),
        _ => {}
    }
}

fn emit_method(
    rel: &str,
    node: Node,
    src: &str,
    path: &[String],
    implicit_public: bool,
    out: &mut Vec<SourceSymbol>,
) {
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
        symbol_exported(modifiers_of(node), implicit_public),
        body_start,
        body_end,
        out,
    );
}

fn emit_ctor(
    rel: &str,
    node: Node,
    src: &str,
    path: &[String],
    implicit_public: bool,
    out: &mut Vec<SourceSymbol>,
) {
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
        symbol_exported(modifiers_of(node), implicit_public),
        body_start,
        body_end,
        out,
    );
}

/// A record's compact constructor (`public Point { ... }`, no parameter list) — same shape as a regular
/// constructor, just with a `block` body field directly instead of `constructor_body`.
fn emit_compact_ctor(
    rel: &str,
    node: Node,
    src: &str,
    path: &[String],
    implicit_public: bool,
    out: &mut Vec<SourceSymbol>,
) {
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
        symbol_exported(modifiers_of(node), implicit_public),
        body_start,
        body_end,
        out,
    );
}

/// `field_declaration` (class/record/enum body) -> `Const` only when `always_const` is `false` AND both
/// `static`+`final` are present; `constant_declaration` (interface/annotation body) -> always `Const`
/// (`always_const: true` — JLS implicit `public static final`). One symbol per comma-separated
/// declarator name, all sharing the declaration's own line — module doc's grouped-declaration rule.
fn emit_fields(
    rel: &str,
    node: Node,
    src: &str,
    path: &[String],
    implicit_public: bool,
    always_const: bool,
    out: &mut Vec<SourceSymbol>,
) {
    let modifiers = modifiers_of(node);
    let is_static_final =
        has_modifier_keyword(modifiers, "static") && has_modifier_keyword(modifiers, "final");
    if !always_const && !is_static_final {
        return; // an instance field — never symbol-surface, module doc.
    }
    let exported = symbol_exported(modifiers, implicit_public);
    let line = line_of(node);
    let mut cursor = node.walk();
    for declarator in node.children_by_field_name("declarator", &mut cursor) {
        if declarator.is_error() || declarator.is_missing() {
            continue;
        }
        let Some(name_node) = declarator.child_by_field_name("name") else {
            continue;
        };
        if name_node.kind() != "identifier" {
            continue; // an underscore-pattern declarator — never guessed, module doc.
        }
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
