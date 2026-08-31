//! The `[Table("…")]` ATTRIBUTE shape — one of the two the parent module doc names, and the one that
//! WINS when both describe the same entity: the literal is the physical table name, so it is used
//! verbatim and the sibling `DbSet` convention name for that entity is suppressed. A non-literal
//! argument skips the class rather than guessing, which is why this arm can emit NOTHING for a class
//! it recognised — and why the suppression it hands the `DbSet` side is keyed on the ATTRIBUTE being
//! present, not on a name having been emitted.

use tree_sitter::Node;
use zzop_core::IoProvide;

use crate::util::{
    attribute_name, attributes_of, line_of, node_text, string_literal_text, valid_named_children,
};

pub(super) fn collect_table_attribute_provides(
    node: Node,
    rel: &str,
    src: &str,
    table_attributed: &mut Vec<String>,
    out: &mut Vec<IoProvide>,
) {
    if node.kind() == "class_declaration" {
        emit_table_attribute(node, rel, src, table_attributed, out);
    }
    for child in valid_named_children(node) {
        collect_table_attribute_provides(child, rel, src, table_attributed, out);
    }
}

fn emit_table_attribute(
    class: Node,
    rel: &str,
    src: &str,
    table_attributed: &mut Vec<String>,
    out: &mut Vec<IoProvide>,
) {
    let Some(attr) = attributes_of(class)
        .into_iter()
        .find(|a| attribute_name(*a, src).as_deref() == Some("Table"))
    else {
        return;
    };
    let Some(name_node) = class.child_by_field_name("name") else {
        return;
    };
    let class_name = node_text(name_node, src);
    // Any [Table] presence suppresses this class's DbSet-convention name (module doc) — recorded
    // before the literal check, so a non-literal rename suppresses without emitting.
    table_attributed.push(class_name.to_string());
    let Some(table) = first_positional_string_literal(attr, src) else {
        return; // nameof(...)/constant/absent — never guessed.
    };
    out.push(IoProvide {
        route_version: None,
        response: None,
        kind: "db-table".to_string(),
        key: format!("table:{}", zzop_core::db_table_channel_casing(&table)),
        file: rel.to_string(),
        line: line_of(name_node),
        symbol: Some(class_name.to_string()),
        body: None,
    });
}

/// The FIRST `attribute_argument`'s string literal, when that is what the argument is — `None` for any
/// other argument shape or an argument-less attribute.
fn first_positional_string_literal(attr: Node, src: &str) -> Option<String> {
    let args = valid_named_children(attr)
        .into_iter()
        .find(|c| c.kind() == "attribute_argument_list")?;
    let first = valid_named_children(args)
        .into_iter()
        .find(|c| c.kind() == "attribute_argument")?;
    let value = valid_named_children(first).into_iter().next()?;
    string_literal_text(value, src)
}
