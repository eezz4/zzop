//! Top-level + NESTED `SourceSymbol` extraction — v1 scope: every `class`/`interface`/`enum`/`record`/
//! `@interface` declaration reachable by walking type-BODY structure only (a type nested inside
//! another type's body), never a LOCAL type/method declared inside a method/constructor BODY (out of
//! scope, mirrors every sibling parser crate's "structural nesting only" convention).
//!
//! ## Qualified naming
//! A nested member's `SourceSymbol::name` is DOT-JOINED with every enclosing type's own simple name
//! (`Outer.Inner`, `Outer.Inner.method`) — extends `zzop_parser_go::lang::symbols`'s `Recv.method`
//! convention one level further (Java nesting can go arbitrarily deep; Go methods attach to exactly one
//! receiver type). A top-level type's name is just its own simple name (empty path prefix).
//!
//! ## Kind mapping (task-pinned)
//! `class`/`enum`/`record` -> `Class`; `interface`/`@interface` (annotation type) -> `Interface`. Neither
//! `enum` nor `record` gets its own `SourceSymbolKind` variant (the same "no dedicated variant" gap
//! `zzop_parser_go::lang::symbols` documents for Go's `const`/`var`), so both collapse onto `Class`.
//!
//! ## `exported`
//! `public`/`protected` -> `true` (task brief: "importable by another in-tree file", and a `protected`
//! member is reachable by a subclass in another package); explicit `private` -> `false` always;
//! otherwise (package-private, no modifier at all) -> `true` ONLY when the enclosing body is an
//! `interface`/`@interface` (JLS 9.4/9.3: an interface member with no explicit access modifier is
//! IMPLICITLY `public`), else `false`. See `symbol_exported`.
//!
//! ## `body_start`/`body_end`
//! For a Class/Interface-kind symbol: `body_start` = the declaration's own START line (== `line`,
//! including any leading annotations — mirrors the OLD lexical crate's `scan.rs` convention exactly,
//! METHOD-SCAN PARITY), `body_end` = the type's `body` node's END line (closing `}`). For a method/
//! constructor: `body_start`/`body_end` = the `body` block's own start/end line, `None`/`None` when no
//! body exists at all (an abstract/interface method declared with `;`).
//!
//! ## Fields
//! `field_declaration` (only valid inside a `class`/`record`/`enum` body) -> `Const` ONLY when BOTH
//! `static` and `final` modifiers are present — an instance field is not symbol-surface (task-pinned).
//! `constant_declaration` (the DISTINCT grammar rule used inside `interface`/`@interface` bodies) is
//! ALWAYS `Const` regardless of written modifiers — JLS 9.3 makes every interface field implicitly
//! `public static final` with no keywords required. A grouped declaration (`static final String A = "x",
//! B = "y";`) emits one symbol per comma-separated name, mirroring
//! `zzop_parser_go::lang::symbols`'s identical "one symbol per NAME within a spec" rule.
//!
//! ## Out of v1 scope (documented, not attempted)
//! Annotation-type ELEMENT declarations (`String value() default "";`) are not extracted as method
//! symbols — structurally a parameterless accessor with an optional default, not a real method. Record
//! components (the implicit accessor/field pair `record Point(int x, int y)` generates) are not
//! extracted either — invisible in source as a written declaration.

mod member;

use tree_sitter::Node;
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::util::{end_line_of, has_modifier_keyword, line_of, modifiers_of, valid_named_children};

/// Extract this file's top-level + nested symbols — see module doc. Empty on parse failure.
pub fn parse_symbols(rel: &str, text: &str) -> Vec<SourceSymbol> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for child in valid_named_children(tree.root_node()) {
        if is_type_decl_kind(child.kind()) {
            emit_type(rel, child, text, &[], false, &mut out);
        }
    }
    out
}

/// `true` for a node kind this crate treats as a Java type declaration — shared with `provides`/
/// `project` (which walk the same class/interface/enum/record/annotation-type NESTING structure this
/// module does, for AST-native enclosing-type recognition instead of the old lexical crate's
/// span-overlap `enclosing_class` search).
pub(crate) fn is_type_decl_kind(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
    )
}

/// Emits `node`'s own `SourceSymbol` (qualified by `path`, the enclosing types' simple names, outermost
/// first) then recurses into its body — module doc.
pub(super) fn emit_type(
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
    let Some(kind) = kind_of(node.kind()) else {
        return;
    };
    let simple_name = crate::util::node_text(name_node, src).to_string();
    let mut qualified_path = path.to_vec();
    qualified_path.push(simple_name);
    let qualified_name = qualified_path.join(".");

    let modifiers = modifiers_of(node);
    let exported = symbol_exported(modifiers, implicit_public);
    let line = line_of(node);

    let Some(body) = node.child_by_field_name("body") else {
        return; // structurally required by the grammar — defensive, never guessed on an ERROR subtree.
    };
    out.push(SourceSymbol {
        id: format!("{rel}#{qualified_name}"),
        file: rel.to_string(),
        name: qualified_name,
        kind,
        line,
        exported,
        is_default: false,
        body_start: Some(line),
        body_end: Some(end_line_of(body)),
        write_sites: Vec::new(),
    });

    let member_implicit_public = matches!(
        node.kind(),
        "interface_declaration" | "annotation_type_declaration"
    );
    emit_body(rel, body, src, &qualified_path, member_implicit_public, out);
}

fn kind_of(node_kind: &str) -> Option<SourceSymbolKind> {
    match node_kind {
        "class_declaration" | "enum_declaration" | "record_declaration" => {
            Some(SourceSymbolKind::Class)
        }
        "interface_declaration" | "annotation_type_declaration" => {
            Some(SourceSymbolKind::Interface)
        }
        _ => None,
    }
}

/// Walks one type's `body` node for member declarations — `enum_body` has an extra wrapper
/// (`enum_body_declarations`, itself holding the same member kinds `class_body`/`interface_body`/
/// `annotation_type_body` hold directly) that must be unwrapped one level; every other body kind's own
/// named children ARE the members directly.
fn emit_body(
    rel: &str,
    body: Node,
    src: &str,
    path: &[String],
    implicit_public: bool,
    out: &mut Vec<SourceSymbol>,
) {
    for child in valid_named_children(body) {
        if child.kind() == "enum_body_declarations" {
            for member in valid_named_children(child) {
                member::emit_member(rel, member, src, path, implicit_public, out);
            }
        } else {
            member::emit_member(rel, child, src, path, implicit_public, out);
        }
    }
}

/// See module doc's `exported` section.
pub(super) fn symbol_exported(modifiers: Option<Node>, implicit_public: bool) -> bool {
    if has_modifier_keyword(modifiers, "public") {
        return true;
    }
    if has_modifier_keyword(modifiers, "protected") {
        return true;
    }
    if has_modifier_keyword(modifiers, "private") {
        return false;
    }
    implicit_public
}

#[cfg(test)]
mod tests;
