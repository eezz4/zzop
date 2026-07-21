//! Top-level + type-NESTED `SourceSymbol` extraction — v1 scope: every `class`/`struct`/`interface`/
//! `record`/`delegate` declaration reachable by walking type-BODY (`declaration_list`) structure only
//! (a type nested inside another type's body), never a LOCAL type/method declared inside a method/
//! constructor BODY — mirrors `zzop_parser_java_21::lang::symbols`'s identical "structural nesting
//! only" convention.
//!
//! ## Namespace transparency
//! A C# `namespace` (block `namespace Foo.Bar { ... }` or file-scoped `namespace Foo.Bar;`) contributes
//! NOTHING to a symbol's qualified name — only TYPE nesting does (module doc below). This crate's
//! top-level walk therefore recurses THROUGH a block namespace's own body transparently (never pushing
//! its name onto the qualification path), while a file-scoped namespace needs no special handling at
//! all: everything that follows it in the file is already a plain top-level sibling of
//! `compilation_unit` by construction (`csharp_namespaces_of` is the fn that reports namespace names
//! themselves, kept entirely separate from symbol qualification).
//!
//! ## Qualified naming
//! A nested member's `SourceSymbol::name` is DOT-JOINED with every enclosing type's own simple name
//! (`Outer.Inner`, `Outer.Inner.Method`) — mirrors `zzop_parser_java_21::lang::symbols`'s
//! `Outer.Inner`/`Outer.Inner.method` convention exactly (C# type nesting, like Java's, can go
//! arbitrarily deep). A top-level type's name is just its own simple name (empty path prefix).
//!
//! ## Kind mapping (task-pinned, judgment calls — `SourceSymbolKind` has no C#-shaped variants either)
//! `class`/`struct`/`record` -> `Class`; `interface` -> `Interface`; `enum` -> `Class` (no dedicated
//! `Enum` variant, the same "no dedicated variant" gap `zzop_parser_go::lang::symbols` documents for
//! Go's `const`/`var` and `zzop_parser_java_21::lang::symbols` documents for Java's `enum`/`record`);
//! `delegate` -> `Type` (a signature-only declaration, the closest analogue to Go's non-struct/
//! interface `type X <T>` spec -> `Type` mapping).
//!
//! ## `exported`
//! A FLAT rule at every level (task-pinned, deliberately simpler than
//! `zzop_parser_java_21::lang::symbols::symbol_exported`'s implicit-public-in-interface nuance):
//! `exported` = has an explicit `public` modifier, full stop. A nested type/member with NO modifier at
//! all (C#'s default: `private` inside a `class`/`struct`/`record`, or implicitly `public` inside an
//! `interface`) is reported `exported: false` either way — including an interface member written with
//! no modifier at all (C#'s own implicit-public rule), a KNOWN scope narrowing vs. Java's nuanced rule,
//! accepted because the task brief pins this literal "has `public` written" gate explicitly. A
//! top-level type with no modifier (C#'s default: `internal`) is likewise NOT exported.
//!
//! ## `body_start`/`body_end`
//! For a Class/Interface-kind symbol (`class`/`struct`/`interface`/`record` with a `declaration_list`
//! body): `body_start` = the declaration's own START line (== `line`, including any leading
//! attributes), `body_end` = the `declaration_list`'s own END line (closing `}`) — mirrors
//! `zzop_parser_java_21::lang::symbols`'s identical type-level body-span convention. A body-less type
//! (a positional-record's compact form `record Point(int X, int Y);`, or a `delegate`, which has no
//! body concept at all) carries `None`/`None`. For a method/constructor: `body_start`/`body_end` = the
//! `block`/`arrow_expression_clause` body's own start/end line, `None`/`None` when no body exists at
//! all (an abstract/interface method declared with `;`).
//!
//! ## Fields and properties (`lang::symbols::member`)
//! `field_declaration` -> `Const` ONLY when the field carries `const`, OR BOTH `static` AND `readonly`
//! — an instance field (or a `static` field without `readonly`) is not symbol-surface (task-pinned,
//! mirrors `zzop_parser_java_21`'s identical `static final`-only field gate). `property_declaration` ->
//! `Const` unconditionally (task-pinned: extract every declared property; `SourceSymbolKind` has no
//! dedicated `Property` variant, so it maps onto the same "declared data member" bucket a field does —
//! the same "nearest existing kind" fallback `zzop_parser_go::lang::symbols` documents for a top-level
//! Go `var`). A grouped field declaration (`public const int A = 1, B = 2;`) emits one symbol per
//! comma-separated declarator name, all sharing the declaration's own line — mirrors
//! `zzop_parser_go`/`zzop_parser_java_21`'s identical "one symbol per NAME within a spec" rule.
//!
//! ## Out of v1 scope (documented, not attempted)
//! Indexers, operator overloads (`operator`/conversion operators), destructors, events, and static
//! constructors contribute no symbol — none maps cleanly onto an existing `SourceSymbolKind`, and none
//! is common enough in the extracted-API-surface census to justify a "nearest kind" guess. Record
//! primary-constructor PARAMETERS (the implicit property/field pair a positional `record Point(int X,
//! int Y)` generates) are likewise not extracted — invisible in source as a written member declaration,
//! the same "not written, not extracted" principle `zzop_parser_java_21::lang::symbols`'s doc pins for
//! Java record components.

mod member;

use tree_sitter::Node;
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::util::{
    end_line_of, has_modifier, line_of, modifiers_of, node_text, valid_named_children,
};

/// Extract this file's top-level + nested symbols — see module doc. Empty on parse failure.
pub fn parse_symbols(rel: &str, text: &str) -> Vec<SourceSymbol> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk_top_level(rel, tree.root_node(), text, &mut out);
    out
}

/// `true` for a node kind this crate treats as a C# type declaration — shared with `adapters::provides`
/// (which walks the same class/struct/interface/record NESTING structure this module does, for
/// AST-native enclosing-type recognition).
pub(crate) fn is_type_decl_kind(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "struct_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "delegate_declaration"
    )
}

/// Recurses THROUGH a block `namespace_declaration`'s own body transparently (module doc's "namespace
/// transparency"), emitting every top-level/namespace-nested type declaration reachable from `node`.
fn walk_top_level(rel: &str, node: Node, src: &str, out: &mut Vec<SourceSymbol>) {
    for child in valid_named_children(node) {
        match child.kind() {
            "namespace_declaration" => {
                if let Some(body) = child.child_by_field_name("body") {
                    walk_top_level(rel, body, src, out);
                }
            }
            k if is_type_decl_kind(k) => emit_type(rel, child, src, &[], out),
            _ => {}
        }
    }
}

/// Emits `node`'s own `SourceSymbol` (qualified by `path`, the enclosing types' simple names, outermost
/// first) then recurses into its body — module doc.
fn emit_type(rel: &str, node: Node, src: &str, path: &[String], out: &mut Vec<SourceSymbol>) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let Some(kind) = kind_of(node.kind()) else {
        return;
    };
    let simple_name = node_text(name_node, src).to_string();
    let mut qualified_path = path.to_vec();
    qualified_path.push(simple_name);
    let qualified_name = qualified_path.join(".");

    let mods = modifiers_of(node);
    let exported = has_modifier(&mods, "public", src);
    let line = line_of(node);
    let body = node.child_by_field_name("body");
    let (body_start, body_end) = match body {
        Some(b) if b.kind() == "declaration_list" => (Some(line), Some(end_line_of(b))),
        _ => (None, None),
    };

    out.push(SourceSymbol {
        id: format!("{rel}#{qualified_name}"),
        file: rel.to_string(),
        name: qualified_name,
        kind,
        line,
        exported,
        is_default: false,
        body_start,
        body_end,
        write_sites: Vec::new(),
    });

    if let Some(b) = body {
        if b.kind() == "declaration_list" {
            emit_body(rel, b, src, &qualified_path, out);
        }
        // `enum_member_declaration_list` holds only bare enum constants — no nested types/methods
        // possible there, module doc's "out of v1 scope" for enum members.
    }
}

fn kind_of(node_kind: &str) -> Option<SourceSymbolKind> {
    match node_kind {
        "class_declaration" | "struct_declaration" | "record_declaration" | "enum_declaration" => {
            Some(SourceSymbolKind::Class)
        }
        "interface_declaration" => Some(SourceSymbolKind::Interface),
        "delegate_declaration" => Some(SourceSymbolKind::Type),
        _ => None,
    }
}

/// Walks one type's `declaration_list` body for member declarations — every member kind
/// (`lang::symbols::member`) is a direct named child; no extra wrapper level (unlike Java's
/// `enum_body_declarations`).
fn emit_body(rel: &str, body: Node, src: &str, path: &[String], out: &mut Vec<SourceSymbol>) {
    for child in valid_named_children(body) {
        if is_type_decl_kind(child.kind()) {
            emit_type(rel, child, src, path, out);
        } else {
            member::emit_member(rel, child, src, path, out);
        }
    }
}

#[cfg(test)]
mod tests;
