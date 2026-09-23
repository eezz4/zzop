//! Identifier-reference collection — dead-export analysis substrate, mirroring
//! `zzop_parser_rust::lang::used_names::parse_local_identifier_refs`'s purpose and "simple name" scope
//! (only the LAST/rightmost segment of a multi-part reference is kept), adapted to tree-sitter-go's
//! kind-disambiguated identifier vocabulary in place of `syn`'s typed `Pat`/`Expr` distinction:
//!
//! - Every bare `identifier` node contributes its text, EXCEPT one sitting in a DECLARATION-introducing
//!   position (module doc's "excluded declaration positions" below) — Rust's analogue is a `Pat::Ident`
//!   binding never being visited as an `Expr`; Go's grammar has no such structural split (an
//!   `identifier` node looks the same whether it's a fresh binding or a read), so this module excludes
//!   those positions explicitly instead.
//! - Every `selector_expression`'s `field` child (a `field_identifier` — e.g. the `Println` in
//!   `fmt.Println(...)`) contributes its text: the "selector rightmost" half of the task brief's
//!   convention, and the direct Go analogue of a Rust `Type::method` `ExprPath`'s last segment.
//! - Every `type_identifier` node contributes its text (a TYPE reference — `var x Foo`, a param/return
//!   type, a `qualified_type`'s own `name` field for `pkg.Type`), mirroring the `syn` crate's `TypePath`
//!   on the Rust side — EXCEPT the declaring occurrence in a `type_spec`/`type_alias`'s own `name` field.
//!
//! ## Excluded declaration positions (never contribute a reference)
//! A `name`-field `identifier` whose parent is `function_declaration`, `method_declaration`'s method
//! name field (a `field_identifier`, already outside this module's `identifier`-kind harvest),
//! `parameter_declaration`, `variadic_parameter_declaration`, `const_spec`, or `var_spec`; a `name`-
//! field `type_identifier` whose parent is `type_spec`/`type_alias`; and every `identifier` on the
//! LEFT side of a `short_var_declaration` (`x := ...`'s `x` — Go's closest equivalent of Rust's `let`
//! binding). A plain re-assignment (`x = 5`, `assignment_statement`) is NOT excluded — like Rust's own
//! bare `x = 5;` (no `let`), it reads an EXISTING binding, matching `zzop_parser_rust`'s own inclusion
//! of an `ExprAssign`'s left side.
//!
//! Out of v1 scope: a `qualified_type`'s `package` field (`package_identifier` kind) and any
//! `package_identifier`/`field_identifier` occurring OUTSIDE the two contexts named above — this
//! module's convention is deliberately narrow (bare value/type NAME segments only), the same "simple
//! name, not full dotted path" scope `zzop_parser_rust::lang::used_names`'s own doc pins.

use std::collections::BTreeSet;
use tree_sitter::{Node, TreeCursor};

use crate::util::node_text;

/// Extract every identifier/type reference (last segment only) in `text`. Empty on parse failure
/// (never panics); a partial in-file error skips just that subtree (crate root doc).
pub fn parse_local_identifier_refs(text: &str) -> BTreeSet<String> {
    let Some(tree) = crate::parse_tree(text) else {
        return BTreeSet::new();
    };
    let mut refs = BTreeSet::new();
    let mut cursor = tree.walk();
    walk(&mut cursor, text, &mut refs, None);
    refs
}

/// The parent NODE is threaded DOWN rather than looked up, and that is a performance contract
/// rather than a style choice (review ledger V131, inherited from V116).
///
/// 📏 The two `is_declared_*` predicates below used to ask `node.parent()` once per identifier.
/// `ts_node_parent` is O(DEPTH) — tree-sitter finds a parent by descending from the root — so the
/// sum over a file is depth x identifiers. Measured on one file with 20,000 identifiers at depth
/// 4,000: **41.8 s** here against **7.2 s** for the C# frontend, which had already been fixed. At
/// 200,000 identifiers this frontend passed 300 s and was killed; C# finished in 62.5 s.
///
/// 🔵 Go carries the NODE where C# and Java carry only the kind, and the difference is real:
/// [`is_short_var_decl_left`] has to read the GRANDparent and compare node ids, which a kind cannot
/// answer. That one lookup stays O(depth) — but it now runs only for an identifier whose parent is
/// an `expression_list`, instead of for every identifier in the file.
fn walk<'a>(
    cursor: &mut TreeCursor<'a>,
    src: &str,
    out: &mut BTreeSet<String>,
    parent: Option<Node<'a>>,
) {
    loop {
        let node = cursor.node();
        if !node.is_error() && !node.is_missing() {
            visit(node, cursor.field_name(), parent, src, out);
            if cursor.goto_first_child() {
                walk(cursor, src, out, Some(node));
                cursor.goto_parent();
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn visit(
    node: Node,
    field: Option<&str>,
    parent: Option<Node>,
    src: &str,
    out: &mut BTreeSet<String>,
) {
    match node.kind() {
        "identifier" if !is_declared_var_name(parent, field) => {
            out.insert(node_text(node, src).to_string());
        }
        "type_identifier" if !is_declared_type_name(parent, field) => {
            out.insert(node_text(node, src).to_string());
        }
        "selector_expression" => {
            if let Some(field_node) = node.child_by_field_name("field") {
                if !field_node.is_error() && !field_node.is_missing() {
                    out.insert(node_text(field_node, src).to_string());
                }
            }
        }
        _ => {}
    }
}

const DECLARES_NAME_FIELD: &[&str] = &[
    "function_declaration",
    "parameter_declaration",
    "variadic_parameter_declaration",
    "const_spec",
    "var_spec",
];

/// See module doc's "excluded declaration positions": a `name`-field declaration, or the LHS of a
/// `short_var_declaration`.
fn is_declared_var_name(parent: Option<Node>, field: Option<&str>) -> bool {
    let Some(parent) = parent else {
        return false;
    };
    if field == Some("name") && DECLARES_NAME_FIELD.contains(&parent.kind()) {
        return true;
    }
    is_short_var_decl_left(parent)
}

/// A node qualifies when its immediate PARENT is the `expression_list` bound as a
/// `short_var_declaration`'s `left` field (module doc) — the field lives on the LIST, not on each
/// individual identifier inside it, so this checks one level up rather than `field` directly. Any
/// `identifier` whose direct parent is that specific list is, by construction, one of the LHS targets
/// (`a := ...` or `a, b := ...`) regardless of its position within it.
fn is_short_var_decl_left(parent: Node) -> bool {
    if parent.kind() != "expression_list" {
        return false;
    }
    let Some(grandparent) = parent.parent() else {
        return false;
    };
    if grandparent.kind() != "short_var_declaration" {
        return false;
    }
    grandparent
        .child_by_field_name("left")
        .is_some_and(|left| left.id() == parent.id())
}

fn is_declared_type_name(parent: Option<Node>, field: Option<&str>) -> bool {
    let Some(parent) = parent else {
        return false;
    };
    field == Some("name") && matches!(parent.kind(), "type_spec" | "type_alias")
}

#[cfg(test)]
mod tests;
