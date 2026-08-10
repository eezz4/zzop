//! One type's own member declarations (methods, constructors, `static final` fields, and nested types)
//! -> `SourceSymbol` — see the parent module doc (`mod.rs`) for the qualified-naming and kind-mapping
//! rules this applies.
//!
//! ## Bodyless methods DO emit a symbol
//! An interface/abstract method emits a `SourceSymbol` with `body_start`/`body_end` = `None` rather
//! than being skipped — the declaration exists and is API surface, so the CST view reports it
//! (`super::tests::interface_abstract_methods_carry_no_body_span`). Consequence worth knowing before
//! reading a census: for a Java tree containing interfaces, the exported-symbol count includes those
//! members, since a modifier-less interface member is implicitly public (`symbol_exported`).

use tree_sitter::Node;
use zzop_core::{SourceSymbol, SourceSymbolKind};

use super::symbol_exported;
use crate::util::{end_line_of, has_modifier_keyword, line_of, modifiers_of, node_text};

/// The member names an initializer block takes. A class may legally hold several of each and they have
/// no key at all, so the 2nd and later take a 1-based ordinal suffix — the exact scheme (and the exact
/// reason for it) `zzop_parser_typescript`'s `symbol_shapes::class::STATIC_BLOCK` states: without the
/// ordinal every block after the first would share one name and one id. The `-` is what makes the
/// spelling collision-proof against a real member: it cannot appear in a Java identifier.
const STATIC_BLOCK: &str = "static-block";
const INSTANCE_BLOCK: &str = "instance-block";

/// Per-type counters for the two anonymous member kinds — one instance per `emit_body` walk, so
/// ordinals restart in each type rather than running across a whole file.
#[derive(Default)]
pub(super) struct BlockOrdinals {
    static_blocks: usize,
    instance_blocks: usize,
}

/// Dispatches one `class_body`/`interface_body`/`annotation_type_body`/`enum_body_declarations` member
/// node by kind — see parent module doc for the recognized shapes; anything else (an annotation-type
/// element, `;`) contributes no symbol.
pub(super) fn emit_member(
    rel: &str,
    node: Node,
    src: &str,
    path: &[String],
    implicit_public: bool,
    ordinals: &mut BlockOrdinals,
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
        "static_initializer" => {
            ordinals.static_blocks += 1;
            emit_init_block(rel, node, path, STATIC_BLOCK, ordinals.static_blocks, out);
        }
        // An instance initializer is a bare `block` sitting directly in a class body.
        "block" => {
            ordinals.instance_blocks += 1;
            emit_init_block(
                rel,
                node,
                path,
                INSTANCE_BLOCK,
                ordinals.instance_blocks,
                out,
            );
        }
        _ => {}
    }
}

/// A `static { … }` / `{ … }` initializer's own leaf — see the parent module doc's leaf-completeness
/// section for why one is owed. `exported: false` always: an initializer is not API surface, it is a
/// region of executable statements that would otherwise be reachable only through the enclosing type's
/// span, which `dsl::method_scan::gates::drop_outer_spans` discards as soon as any sibling member
/// projects a leaf of its own.
fn emit_init_block(
    rel: &str,
    node: Node,
    path: &[String],
    stem: &str,
    ordinal: usize,
    out: &mut Vec<SourceSymbol>,
) {
    let own_name = match ordinal {
        1 => stem.to_string(),
        n => format!("{stem}-{n}"),
    };
    push(
        rel,
        path,
        &own_name,
        SourceSymbolKind::Function,
        line_of(node),
        false,
        Some(line_of(node)),
        Some(end_line_of(node)),
        out,
    );
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
    // `body_start` is the DECLARATION's own line — annotations and modifiers included, since `node` is
    // the whole `*_declaration` — never the body block's `{`. See `zzop_core::SourceSymbol`'s "Body
    // span contract". No `body` at all (an abstract/interface method) keeps `None`/`None`.
    let (body_start, body_end) = node
        .child_by_field_name("body")
        .map(|b| (Some(line_of(node)), Some(end_line_of(b))))
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
    // `body_start` is the DECLARATION's own line — annotations and modifiers included, since `node` is
    // the whole `*_declaration` — never the body block's `{`. See `zzop_core::SourceSymbol`'s "Body
    // span contract". No `body` at all (an abstract/interface method) keeps `None`/`None`.
    let (body_start, body_end) = node
        .child_by_field_name("body")
        .map(|b| (Some(line_of(node)), Some(end_line_of(b))))
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
    // `body_start` is the DECLARATION's own line — annotations and modifiers included, since `node` is
    // the whole `*_declaration` — never the body block's `{`. See `zzop_core::SourceSymbol`'s "Body
    // span contract". No `body` at all (an abstract/interface method) keeps `None`/`None`.
    let (body_start, body_end) = node
        .child_by_field_name("body")
        .map(|b| (Some(line_of(node)), Some(end_line_of(b))))
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
