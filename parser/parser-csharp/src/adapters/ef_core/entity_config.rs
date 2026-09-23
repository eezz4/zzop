//! The FLUENT `ToTable("…")` shape inside an `IEntityTypeConfiguration<T>` class — the third shape the
//! parent module doc names, and the one that carries the PHYSICAL table name in every hand-written EF
//! Core mapping this repo has measured.
//!
//! Why this arm exists at all, with the census beside it: across the three C# trees measured on
//! 2026-09-11, `[Table(` occurs in **0** files (eShop 0 / be-aspnet 0 / aspnetcore 0) while
//! `IEntityTypeConfiguration<` occurs in **9 / 0 / 0** and `ToTable(` on **81 / 0 / 191** lines. The
//! declarative attribute arm reaches nothing; the fluent configuration class is what .NET actually
//! writes, and the `DbSet<T>` convention name it OVERRIDES is what this build was emitting in its
//! place — not a missing fact but a WRONG one (eShop keyed `table:payments` where the database has
//! `paymentmethods`, `table:catalogItems` where it has `Catalog`).
//!
//! ENTITY RESOLUTION NEEDS NO TYPE RESOLVER, which is what makes the shape cheap: the entity is fixed
//! by the CLASS's own base-list type argument (`class CatalogItemEntityTypeConfiguration :
//! IEntityTypeConfiguration<CatalogItem>`), never by the `ToTable` receiver — which is a
//! per-file-arbitrary parameter name (`builder`, `paymentConfiguration`, `orderConfiguration`, … in
//! eShop's 9 files) and is deliberately not read here.
//!
//! SCOPE, and each bound is a never-guess refusal rather than an omission:
//! - Only a class whose OWN base list names `IEntityTypeConfiguration<T>` with a resolvable simple `T`.
//! - Only the FIRST `ToTable` call reachable inside that class, and only when its first argument is a
//!   plain `string_literal`. A `nameof(...)`/constant/interpolated argument emits nothing.
//! - The `ModelBuilder.Entity<T>(b => { b.ToTable("…"); })` LAMBDA overload is a separate shape and is
//!   NOT read here (eShop 1 site, `IntegrationLogExtensions.cs:9`; aspnetcore's hand-written 22 are all
//!   that shape, over OPEN generic parameters `TUser`/`TRole`). Deliberately deferred — see the parent
//!   module doc.
//! - NO CROSS-FILE SUPPRESSION. eShop puts the configuration classes in `EntityConfigurations/*.cs` and
//!   the `DbSet<T>` properties in `CatalogContext.cs`, and per-file io projection cannot let one file's
//!   facts silence another's. So the convention-named `DbSet` provides SURVIVE beside the correct
//!   fluent ones: after this arm eShop emits both `table:paymentmethods` (real) and `table:payments`
//!   (phantom). That is a deliberate staged result, stated here because it is invisible otherwise.

use tree_sitter::Node;
use zzop_core::IoProvide;

use crate::util::{line_of, node_text, string_literal_text, valid_named_children};

use super::dbset::type_simple_name;

/// The base-list entry this arm keys on. Matched on the leading token, so an explicitly qualified
/// `Microsoft.EntityFrameworkCore.IEntityTypeConfiguration<T>` is NOT matched — it does not occur in any
/// measured tree and guessing at a qualified spelling would widen the gate past what was measured.
const ENTITY_CONFIG_BASE: &str = "IEntityTypeConfiguration";

/// True when some class in this file implements `IEntityTypeConfiguration<…>` — the FILE-level half of
/// this arm's gate, the sibling of [`super::declares_db_context`]. It exists for the same reason that
/// one does: C# 10's `global using` leaves these files with ZERO `using` lines (measured: all 9 eShop
/// configuration files), so no import gate can reach them, and the base list is the evidence that
/// replaces it — EF Core REQUIRES that interface for the class to be a configuration at all.
pub(super) fn declares_entity_config(node: Node, src: &str) -> bool {
    if node.kind() == "class_declaration" && entity_config_argument(node, src).is_some() {
        return true;
    }
    valid_named_children(node)
        .into_iter()
        .any(|c| declares_entity_config(c, src))
}

/// Walk the file emitting one `db-table` provide per `IEntityTypeConfiguration<T>` class that names its
/// table with a literal `ToTable`.
pub(super) fn collect_entity_config_provides(
    node: Node,
    rel: &str,
    src: &str,
    out: &mut Vec<IoProvide>,
) {
    if node.kind() == "class_declaration" {
        if let Some(entity) = entity_config_argument(node, src) {
            emit_entity_config(node, rel, src, &entity, out);
        }
    }
    for child in valid_named_children(node) {
        collect_entity_config_provides(child, rel, src, out);
    }
}

fn emit_entity_config(class: Node, rel: &str, src: &str, entity: &str, out: &mut Vec<IoProvide>) {
    let Some((table, line)) = first_to_table_literal(class, src) else {
        return; // no ToTable, or a non-literal one — the class maps by convention, nothing to add.
    };
    out.push(IoProvide {
        route_version: None,
        response: None,
        kind: "db-table".to_string(),
        key: format!("table:{}", zzop_core::db_table_channel_casing(&table)),
        file: rel.to_string(),
        line,
        symbol: Some(entity.to_string()),
        body: None,
    });
}

/// `T` from THIS class's own `IEntityTypeConfiguration<T>` base entry, as a simple name; `None` when the
/// class implements no such interface or when `T` names no single type (a nested generic, a tuple).
fn entity_config_argument(class: Node, src: &str) -> Option<String> {
    valid_named_children(class)
        .into_iter()
        .filter(|c| c.kind() == "base_list")
        .flat_map(valid_named_children)
        .find_map(|b| generic_argument_of(b, ENTITY_CONFIG_BASE, src))
}

/// The single type argument of a `generic_name` base entry whose head identifier is `head_name`.
fn generic_argument_of(base: Node, head_name: &str, src: &str) -> Option<String> {
    if base.kind() != "generic_name" {
        return None;
    }
    let mut children = valid_named_children(base).into_iter();
    let head = children.next()?;
    if node_text(head, src) != head_name {
        return None;
    }
    let args = children.find(|c| c.kind() == "type_argument_list")?;
    let mut targs = valid_named_children(args).into_iter();
    let first = targs.next()?;
    if targs.next().is_some() {
        return None; // `IEntityTypeConfiguration` takes exactly one — anything else is not it.
    }
    type_simple_name(first, src)
}

/// The first `ToTable("literal")` call reachable inside `class`, with its 1-based line. `None` when the
/// class calls no `ToTable`, or calls it with anything but a plain string literal first argument.
///
/// FIRST rather than every match: EF Core maps one entity per configuration class to one table, so a
/// second `ToTable` in the same class belongs to an owned/split entity this arm does not model. Taking
/// the first keeps the primary mapping and refuses to invent a second table for the same entity.
fn first_to_table_literal(class: Node, src: &str) -> Option<(String, u32)> {
    if class.kind() == "invocation_expression" {
        if let Some(found) = to_table_literal(class, src) {
            return Some(found);
        }
    }
    valid_named_children(class)
        .into_iter()
        .find_map(|c| first_to_table_literal(c, src))
}

/// `<anything>.ToTable("name")` -> `("name", line)`. The RECEIVER is deliberately unread (module doc).
fn to_table_literal(call: Node, src: &str) -> Option<(String, u32)> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "member_access_expression" {
        return None;
    }
    let name_node = func.child_by_field_name("name")?;
    if node_text(name_node, src) != "ToTable" {
        return None;
    }
    let args = call.child_by_field_name("arguments")?;
    let first = valid_named_children(args)
        .into_iter()
        .find(|a| a.kind() == "argument")?;
    let expr = valid_named_children(first).into_iter().next()?;
    let table = string_literal_text(expr, src)?; // non-literal -> skip, never guessed
    Some((table, line_of(call)))
}
