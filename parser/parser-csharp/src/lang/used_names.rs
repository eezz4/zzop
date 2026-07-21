//! Identifier-reference collection — dead-export analysis substrate, mirroring
//! `zzop_parser_go`/`zzop_parser_java_21::lang::used_names`'s purpose and "simple name, rightmost
//! segment only" scope, adapted to `tree-sitter-c-sharp`'s vocabulary:
//!
//! - Every bare `identifier` node contributes its text, EXCEPT one sitting in a DECLARATION-introducing
//!   `name` field (module doc's "excluded declaration positions" below). Unlike Go/Java, C# has no
//!   separate `type_identifier`/`field_identifier` kind — a type reference, a value reference, and a
//!   `member_access_expression`'s own member name are ALL plain `identifier` nodes, so ONE rule covers
//!   every case Go/Java need two or three rules for (a `member_access_expression`'s `name` field needs
//!   no special-casing at all: it is already a plain `identifier` in a non-excluded position, the same
//!   free ride `zzop_parser_java_21::lang::used_names`'s own doc notes for Java's `field_access`).
//! - A `qualified_name` (`a.b.C`) and an `alias_qualified_name` (`a::b`) each contribute ONLY their own
//!   RIGHTMOST `name` field and are NOT further descended into — otherwise every interior
//!   namespace/qualifier segment (itself a plain `identifier` node) would independently surface via the
//!   general rule above, over-collecting the full dotted chain instead of "rightmost segment only"
//!   (task-pinned convention, mirrors `zzop_parser_java_21::lang::used_names`'s identical
//!   `scoped_identifier` treatment). This grammar reserves `qualified_name` for DECLARATIVE/TYPE-
//!   reference positions (a `using` target, a variable/parameter/return TYPE, an attribute name, a
//!   namespace name) — a RUNTIME member-access chain (`System.Console.WriteLine(...)`) is instead
//!   NESTED `member_access_expression` all the way down (verified empirically against the compiled
//!   grammar), which needs no special-casing at all (this module's first bullet's "free ride"): EVERY
//!   segment of such a chain is its own genuine reference and is collected, the same behavior
//!   `zzop_parser_java_21::lang::used_names`'s own doc documents for a chained Java `field_access`.
//!
//! ## Excluded declaration positions (never contribute a reference)
//! A `name`-field `identifier` whose parent is a type declaration (`class_declaration`/
//! `struct_declaration`/`interface_declaration`/`record_declaration`/`enum_declaration`/
//! `delegate_declaration`), a member declaration (`method_declaration`/`constructor_declaration`/
//! `property_declaration`), a `parameter`, a `variable_declarator` (covers local vars, instance/static
//! fields alike — all share this one grammar rule), an `enum_member_declaration`, or a `using_directive`
//! ALIAS name (`using X = ...;`'s `X` — the binding being introduced, not a read).
//!
//! Out of v1 scope (documented, not attempted — the same spirit as `zzop_parser_go`/
//! `zzop_parser_java_21`'s own narrow declared-position sets): a lambda's `implicit_parameter`, a
//! `catch_declaration`/`foreach_statement` loop variable's own name, and a generic `type_parameter`'s
//! own name — each is a DECLARING occurrence this module does not special-case out, so it may surface
//! as a (harmless, low-signal) false-positive "used name". A namespace's own dotted name (in
//! `namespace_declaration`/`file_scoped_namespace_declaration`) is likewise NOT excluded — its rightmost
//! segment surfaces as a used name, the same harmless over-collection
//! `zzop_parser_java_21::lang::used_names`'s doc accepts for Java's own `package_declaration`.

use std::collections::BTreeSet;
use tree_sitter::{Node, TreeCursor};

use crate::util::node_text;

/// Parent kinds whose own `name`-field child is a DECLARATION, never a read — module doc.
const DECLARES_NAME_FIELD: &[&str] = &[
    "class_declaration",
    "struct_declaration",
    "interface_declaration",
    "record_declaration",
    "enum_declaration",
    "delegate_declaration",
    "method_declaration",
    "constructor_declaration",
    "property_declaration",
    "parameter",
    "variable_declarator",
    "enum_member_declaration",
    "using_directive",
];

/// Extract every identifier/type reference (rightmost segment only) in `text` — a FULL valid-region CST
/// walk. Empty on parse failure (never panics); a partial in-file error skips just that subtree.
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
/// `qualified_name`/`alias_qualified_name` "rightmost segment only, no further descent" rule).
fn visit(node: Node, field: Option<&str>, src: &str, out: &mut BTreeSet<String>) -> bool {
    match node.kind() {
        "identifier" if !is_declared_name(node, field) => {
            out.insert(node_text(node, src).to_string());
            false
        }
        "qualified_name" | "alias_qualified_name" => {
            if let Some(name) = node.child_by_field_name("name") {
                if let Some(simple) = simple_name_text(name, src) {
                    out.insert(simple);
                }
            }
            true
        }
        _ => false,
    }
}

/// The leading `identifier` of a `_simple_name` node (`identifier` directly, or a `generic_name`'s own
/// leading identifier) — the rightmost-segment text a `qualified_name`/`alias_qualified_name`'s own
/// `name` field resolves to.
fn simple_name_text(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(node_text(node, src).to_string()),
        "generic_name" => {
            let mut cursor = node.walk();
            let found = node
                .named_children(&mut cursor)
                .find(|c| c.kind() == "identifier")
                .map(|n| node_text(n, src).to_string());
            found
        }
        _ => None,
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
