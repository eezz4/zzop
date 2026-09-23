//! The `DbSet<T>` PROPERTY shape — the other of the two the parent module doc names, and the
//! CONVENTIONAL one: EF maps the entity to a table named after the property, so the property name
//! keys the provide while `symbol` carries `T`'s simple name for the engine's entity-consume join.
//! It yields to the `[Table]` arm for any entity declared in THIS file (see that module), and the
//! cross-file rename it cannot see is the documented inert-provide limit in the parent doc.

use tree_sitter::Node;
use zzop_core::IoProvide;

use crate::util::{line_of, node_text, valid_named_children};

use super::class_derives_db_context;

/// `gated` is whether the NEAREST ENCLOSING declaration may contribute. It is recomputed on entering
/// every class rather than inherited from the file, because the structural signal belongs to ONE class:
/// a file holding `class AppDbContext : DbContext` alongside an unrelated `class Snapshot` carrying a
/// `DbSet<Audit>` property would otherwise emit `table:audits` on the context's behalf.
///
/// `ef_import` short-circuits the whole narrowing, and that ordering is load-bearing rather than
/// stylistic. Keying the recomputation on `class_declaration` ALONE silently dropped every `DbSet<T>`
/// declared in an `interface`/`record`/`struct` — such a property reaches no `class_declaration` on the
/// way down, so `gated` stayed at its `false` seed even with the `using` present. That cost the provides
/// on `interface IApplicationDbContext { DbSet<TodoList> TodoLists { get; } }`, which is the .NET
/// Clean-Architecture template's own shape, not a synthetic one. The narrowing exists to stop ONE class
/// speaking for another under the structural signal; where the import already licenses the file, there
/// is nothing to narrow.
///
/// The structural path still recognises only `class_declaration` contexts, because
/// `class_derives_db_context` reads a class's base list — a `record` deriving from `DbContext` is legal
/// and not idiomatic, and it extracts nothing here rather than being guessed at.
pub(super) fn collect_dbset_provides(
    node: Node,
    rel: &str,
    src: &str,
    ef_import: bool,
    gated: bool,
    table_attributed: &[String],
    out: &mut Vec<IoProvide>,
) {
    let gated = if ef_import {
        true
    } else if node.kind() == "class_declaration" {
        class_derives_db_context(node, src)
    } else {
        gated
    };
    if gated && node.kind() == "property_declaration" {
        emit_dbset(node, rel, src, table_attributed, out);
    }
    for child in valid_named_children(node) {
        collect_dbset_provides(child, rel, src, ef_import, gated, table_attributed, out);
    }
}

fn emit_dbset(
    prop: Node,
    rel: &str,
    src: &str,
    table_attributed: &[String],
    out: &mut Vec<IoProvide>,
) {
    let Some(ty) = prop.child_by_field_name("type") else {
        return;
    };
    let Some(entity) = dbset_entity_name(ty, src) else {
        return;
    };
    if table_attributed.iter().any(|c| c == &entity) {
        return; // same-file [Table] override wins (module doc).
    }
    let Some(name_node) = prop.child_by_field_name("name") else {
        return;
    };
    let prop_name = node_text(name_node, src);
    out.push(IoProvide {
        route_version: None,
        response: None,
        kind: "db-table".to_string(),
        key: format!("table:{}", zzop_core::db_table_channel_casing(prop_name)),
        file: rel.to_string(),
        line: line_of(name_node),
        symbol: Some(entity),
        body: None,
    });
}

/// `DbSet<T>` (optionally `DbSet<T>?`) -> `T`'s simple name; `None` for any other property type.
fn dbset_entity_name(ty: Node, src: &str) -> Option<String> {
    let ty = if ty.kind() == "nullable_type" {
        valid_named_children(ty).into_iter().next()?
    } else {
        ty
    };
    if ty.kind() != "generic_name" {
        return None;
    }
    let mut children = valid_named_children(ty).into_iter();
    let head = children.next()?;
    if node_text(head, src) != "DbSet" {
        return None;
    }
    let args = children.find(|c| c.kind() == "type_argument_list")?;
    let arg = valid_named_children(args).into_iter().next()?;
    type_simple_name(arg, src)
}

/// The simple (rightmost-segment) name of a type-argument node: `User`, `Models.User` -> `User`;
/// `None` for a shape that names no single entity type (a nested generic, a tuple, ...).
///
/// Shared with the sibling `entity_config` arm, which resolves its entity from a BASE-LIST type
/// argument rather than a property type — the same question about the same node kinds, so the same
/// answer rather than a second copy of it.
pub(super) fn type_simple_name(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(node_text(node, src).to_string()),
        "qualified_name" => {
            let last = node.child_by_field_name("name")?;
            type_simple_name(last, src)
        }
        _ => None,
    }
}
