//! EF Core `DbSet<T>`/`[Table]` -> `db-table` PROVIDE extraction (the C# member of the ORM db-table
//! family, alongside `zzop_parser_java_21::jpa`, `zzop_parser_go::adapters::gorm`, the TypeScript
//! TypeORM adapters and the Prisma/SQL provide sides). Two declarative shapes, each behind ITS OWN
//! import gate (a file importing neither yields nothing):
//!
//! - **`[Table("…")]` attribute** on a class (gate: `using System.ComponentModel.DataAnnotations.Schema;`
//!   — [`TABLE_ATTRIBUTE_SPECIFIERS`]): the string literal IS the physical table name, used verbatim. A
//!   NON-LITERAL argument (`nameof(...)`, a constant) skips that class entirely — never guessed.
//! - **`DbSet<T>` property** (gate: `using Microsoft.EntityFrameworkCore;` — [`EF_CORE_SPECIFIERS`]):
//!   EF Core's convention maps the entity to a table named after the DbSet PROPERTY (`DbSet<User>
//!   Users` -> table `Users`), so the property name keys the provide and `symbol` carries `T`'s simple
//!   name (the engine's `resolve_orm_entity_consumes` mechanism, identical to GORM/TypeORM/JPA). A
//!   `[Table]` attribute on the entity class OVERRIDES that convention, so a `DbSet<T>` whose `T` is
//!   declared IN THIS FILE with a `[Table]` attribute is suppressed (the attribute side already emitted
//!   the correct name, or deliberately emitted nothing for a non-literal). Documented cross-file limit:
//!   an entity class living in ANOTHER file with a `[Table]` rename still gets the convention-named
//!   DbSet provide here — per-file extraction cannot see the rename; such a provide simply never joins
//!   (inert), the same acceptance `gorm`'s wide net documents.
//!
//! Deliberately NOT recognized (disclosed): fluent `modelBuilder.Entity<T>().ToTable("…")` mapping
//! (a method-call shape inside `OnModelCreating`, not a declarative annotation — roadmap), and a
//! CONSUME side (`context.Users.Where(...)` query sites) — same one-side-at-a-time shape the JPA arm
//! ships with.
//!
//! Test surface: gated on the shared `zzop_core::is_test_file` PATH predicate (C# has no inline
//! in-source test idiom, so a path gate is the right axis). Since 2026-08-03 that predicate carries
//! the C# conventions itself — `FooTests.cs`/`FooTest.cs` file names and `MyApp.Tests/` project
//! directories — so a conventional C# test project's provides are excluded here at extraction, and
//! the sibling `http_clients` egress adapter (which deliberately carries no gate of its own) has its
//! test-project consumes dropped by the engine's cross-layer join filter, which reads the same
//! predicate. One shared arm, never a per-adapter suffix vocabulary.

use tree_sitter::Node;
use zzop_core::IoProvide;

use crate::util::{
    attribute_name, attributes_of, line_of, node_text, string_literal_text, valid_named_children,
};

const EF_CORE_SPECIFIERS: &[&str] = &["Microsoft.EntityFrameworkCore"];

const TABLE_ATTRIBUTE_SPECIFIERS: &[&str] = &["System.ComponentModel.DataAnnotations.Schema"];

/// Extract this file's EF Core `db-table` provides — see module doc. Empty on parse failure, on a
/// test-classified path, and whenever the file carries neither gating `using` (never panics).
pub fn extract_ef_core_db_table_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    if zzop_core::is_test_file(rel) {
        return Vec::new();
    }
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    let gate = |specs: &[&str]| {
        imports
            .values()
            .any(|b| specs.contains(&b.specifier.as_str()))
    };
    let ef = gate(EF_CORE_SPECIFIERS);
    let table_attr = gate(TABLE_ATTRIBUTE_SPECIFIERS);
    if !ef && !table_attr {
        return Vec::new();
    }
    let root = tree.root_node();
    let mut out = Vec::new();
    // Pass 1 — `[Table]`-attributed classes: emits the attribute-named provides AND collects the
    // suppression set for pass 2 (every class carrying the attribute at all, literal or not).
    let mut table_attributed: Vec<String> = Vec::new();
    if table_attr {
        collect_table_attribute_provides(root, rel, text, &mut table_attributed, &mut out);
    }
    // Pass 2 — `DbSet<T>` properties (convention naming, minus the same-file overrides above).
    if ef {
        collect_dbset_provides(root, rel, text, &table_attributed, &mut out);
    }
    out
}

// --- [Table] attribute side ---------------------------------------------------------------------------

fn collect_table_attribute_provides(
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

// --- DbSet<T> property side ---------------------------------------------------------------------------

fn collect_dbset_provides(
    node: Node,
    rel: &str,
    src: &str,
    table_attributed: &[String],
    out: &mut Vec<IoProvide>,
) {
    if node.kind() == "property_declaration" {
        emit_dbset(node, rel, src, table_attributed, out);
    }
    for child in valid_named_children(node) {
        collect_dbset_provides(child, rel, src, table_attributed, out);
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
fn type_simple_name(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(node_text(node, src).to_string()),
        "qualified_name" => {
            let last = node.child_by_field_name("name")?;
            type_simple_name(last, src)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
