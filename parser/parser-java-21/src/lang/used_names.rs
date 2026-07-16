//! Identifier-reference collection — dead-export analysis substrate, mirroring
//! `zzop_parser_go::lang::used_names::parse_local_identifier_refs`'s purpose and "simple name, rightmost
//! segment only" scope, adapted to `tree-sitter-java`'s vocabulary:
//!
//! - Every bare `identifier` node contributes its text, EXCEPT one sitting in a DECLARATION-introducing
//!   `name` field (module doc's "excluded declaration positions" below).
//! - Every `type_identifier` node contributes its text (a TYPE reference — `String s`, `List<Foo>`,
//!   `extends Base` — Java's grammar aliases a bare type-reference identifier to this kind, distinct
//!   from a value-reference `identifier`).
//! - A `scoped_identifier` (`a.b.C` in a package/import/annotation-name position) and a
//!   `scoped_type_identifier` (`a.b.C` in a type-reference position) each contribute ONLY their own
//!   RIGHTMOST segment and are NOT further descended into — otherwise every interior package/scope
//!   segment (each itself a plain `identifier`/`type_identifier` node) would independently surface via
//!   the two general rules above, over-collecting the full dotted chain instead of "rightmost segment
//!   of scoped identifiers" (task-pinned convention).
//! - `field_access` (`obj.field`) and `method_invocation` (`obj.method(...)`/`method(...)`) need NO
//!   special-casing at all: their `field`/`name` child is already a plain `identifier` node in a
//!   non-excluded position, so the general bare-`identifier` rule picks it up for free, and the walk
//!   continues normally into `object` so a chained/nested read (`getFoo().bar`) is not lost.
//!
//! ## Excluded declaration positions (never contribute a reference)
//! A `name`-field `identifier` whose parent is a type declaration (`class_declaration`/
//! `interface_declaration`/`enum_declaration`/`record_declaration`/`annotation_type_declaration`), a
//! member declaration (`method_declaration`/`constructor_declaration`/`compact_constructor_declaration`),
//! a `formal_parameter`, a `variable_declarator` (covers local vars, instance/static fields, and
//! interface/annotation constants alike — all share this one grammar rule), or an `enum_constant`.
//!
//! Out of v1 scope (documented, not attempted — the same spirit as `zzop_parser_go`'s own narrow
//! declared-position set): a lambda parameter's bare identifier, a generic `type_parameter`'s own
//! (unfielded) `type_identifier`, and a catch/resource variable's name — each is a DECLARING occurrence
//! this module does not special-case out, so it may surface as a (harmless, low-signal) false-positive
//! "used name".

use std::collections::BTreeSet;
use tree_sitter::{Node, TreeCursor};

use crate::util::node_text;

/// Parent kinds whose own `name`-field child is a DECLARATION, never a read — module doc.
const DECLARES_NAME_FIELD: &[&str] = &[
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
    "method_declaration",
    "constructor_declaration",
    "compact_constructor_declaration",
    "formal_parameter",
    "variable_declarator",
    "enum_constant",
];

/// Extract every identifier/type reference (rightmost segment only) in `text` — a FULL valid-region CST
/// walk (unlike `lang::symbols`'s structural top-level+nested-type-only scope). Empty on parse failure
/// (never panics); a partial in-file error skips just that subtree.
pub fn parse_local_identifier_refs(text: &str) -> BTreeSet<String> {
    let Some(tree) = crate::parse_tree(text) else {
        return BTreeSet::new();
    };
    let mut refs = BTreeSet::new();
    let mut cursor = tree.walk();
    walk(&mut cursor, text, &mut refs);
    refs
}

fn walk(cursor: &mut TreeCursor, src: &str, out: &mut BTreeSet<String>) {
    loop {
        let node = cursor.node();
        if !node.is_error() && !node.is_missing() {
            let stop_here = visit(node, cursor.field_name(), src, out);
            if !stop_here && cursor.goto_first_child() {
                walk(cursor, src, out);
                cursor.goto_parent();
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Visits one node, returning `true` when the walk must NOT descend into its children (the
/// `scoped_identifier`/`scoped_type_identifier` "rightmost segment only, no further descent" rule).
fn visit(node: Node, field: Option<&str>, src: &str, out: &mut BTreeSet<String>) -> bool {
    match node.kind() {
        "identifier" if !is_declared_name(node, field) => {
            out.insert(node_text(node, src).to_string());
            false
        }
        "type_identifier" => {
            out.insert(node_text(node, src).to_string());
            false
        }
        "scoped_identifier" => {
            if let Some(name) = node.child_by_field_name("name") {
                out.insert(node_text(name, src).to_string());
            }
            true
        }
        "scoped_type_identifier" => {
            if let Some(name) = crate::util::simple_type_name(node, src) {
                out.insert(name);
            }
            true
        }
        _ => false,
    }
}

/// See module doc's "excluded declaration positions".
fn is_declared_name(node: Node, field: Option<&str>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    field == Some("name") && DECLARES_NAME_FIELD.contains(&parent.kind())
}

#[cfg(test)]
mod tests;
