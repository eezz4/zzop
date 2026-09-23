//! EF Core `DbSet<T>`/`[Table]` -> `db-table` PROVIDE extraction (the C# member of the ORM db-table
//! family, alongside `zzop_parser_java_21::jpa`, `zzop_parser_go::adapters::gorm`, the TypeScript
//! TypeORM adapters and the Prisma/SQL provide sides). THREE shapes, each behind ITS OWN gate (a file
//! matching none yields nothing):
//!
//! - **`[Table("…")]` attribute** on a class (gate: `using System.ComponentModel.DataAnnotations.Schema;`
//!   — [`TABLE_ATTRIBUTE_SPECIFIERS`]): the string literal IS the physical table name, used verbatim. A
//!   NON-LITERAL argument (`nameof(...)`, a constant) skips that class entirely — never guessed.
//! - **`DbSet<T>` property** (gate: `using Microsoft.EntityFrameworkCore;` — [`EF_CORE_SPECIFIERS`] —
//!   OR a class in the file deriving from a `…DbContext` base, since C# 10's `global using` moves the
//!   import to one collector file and leaves every context declaring none; see
//!   [`declares_db_context`]):
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
//! - **fluent `ToTable("…")` inside an `IEntityTypeConfiguration<T>` class** (gate: that base list — see
//!   [`entity_config`]): the string literal IS the physical table name, and the entity comes from the
//!   class's own base-list type argument, so no type resolver is needed. This is the shape .NET
//!   actually writes: measured 2026-09-11 over three C# trees, `[Table(` reaches **0** files in all
//!   three while `IEntityTypeConfiguration<` reaches 9 (eShop) and `ToTable(` 81/0/191 lines. It
//!   OVERRIDES the `DbSet<T>` convention name in EF, but this build cannot suppress the losing provide:
//!   the two shapes sit in different FILES and io projection is per-file, so both keys emit — the
//!   `entity_config` module doc states the consequence in full.
//!
//! Deliberately NOT recognized (disclosed): the fluent LAMBDA overload
//! `modelBuilder.Entity<T>(b => { b.ToTable("…"); })` inside `OnModelCreating` — a distinct shape whose
//! `T` is an open generic parameter (`TUser`/`TRole`) in every measured site, so its `symbol` would key
//! the entity-consume index on a type parameter rather than an entity; the chained
//! `modelBuilder.Entity<T>().ToTable("…")` spelling, which occurs **0** times in eShop and aspnetcore;
//! the SCHEMA segment (`modelBuilder.HasDefaultSchema("ordering")` — io keys carry no schema segment in
//! ANY language, so a perfect ToTable read still yields `table:orders`, never `ordering.orders`); and a
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

use crate::util::{node_text, valid_named_children};

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
    let root = tree.root_node();
    // The `using` gate alone stopped working when C# 10 shipped `global using`: a project collects its
    // framework imports into one `GlobalUsings.cs` and every other file declares none. Measured on
    // dotnet/eShop, 28 files reference `Microsoft.EntityFrameworkCore` and the FIRST is that collector,
    // while `CatalogContext.cs` opens with `namespace …;` and carries no using at all — so all 9
    // `DbSet<T>` properties across its 3 contexts extracted zero.
    //
    // The second gate is the file's OWN declaration and is stronger than the import it replaces: a class
    // whose base list names a type ending in `DbContext` IS an EF Core context, by the framework's own
    // required inheritance, and no import can make that untrue or absent.
    //
    // It gates BOTH arms, and the first version of this change gated only `DbSet<T>`. The argument for
    // that narrower version was that `[Table]` keys on an attribute NAME a non-EF library could also
    // spell, so widening it would be guessing — that argument is recorded here because it is wrong in a
    // way worth keeping: it prices the risk of an EXTRA provide while the actual cost was a WRONG one.
    // See the `table_attr` line below for the measurement that settled it.
    let ef_import = gate(EF_CORE_SPECIFIERS);
    let structural = declares_db_context(root, text);
    let ef = ef_import || structural;
    // `table_attr` takes the SAME structural signal, and leaving it out was a defect rather than a
    // narrowing: `global using` moves BOTH namespaces into one collector file, so under the very layout
    // this gate was widened for, pass 1 stopped running while pass 2 kept going — and pass 1 is what
    // collects the `[Table]` override set pass 2 consults. Measured: a file carrying
    // `[Table("app_users")] class User` and `class AppDbContext : DbContext` with no using of its own
    // emitted `table:users`. Not a missing fact — a WRONG one, at a table name the database does not
    // have, in the one file layout modern .NET writes.
    let table_attr = gate(TABLE_ATTRIBUTE_SPECIFIERS) || structural;
    // The fluent arm's gate is its OWN structural signal and is deliberately NOT folded into `ef` or
    // `table_attr`: a configuration class declares no `DbSet<T>` and carries no `[Table]`, so widening
    // either of those gates with it would open files to passes that have nothing to read there. It is
    // the arm the corpus census says is the one that matters — see `entity_config`'s module doc for the
    // numbers ([Table( reaches 0 files in all three measured C# trees).
    let entity_config = entity_config::declares_entity_config(root, text);
    if !ef && !table_attr && !entity_config {
        return Vec::new();
    }
    let mut out = Vec::new();
    // Pass 1 — `[Table]`-attributed classes: emits the attribute-named provides AND collects the
    // suppression set for pass 2 (every class carrying the attribute at all, literal or not).
    let mut table_attributed: Vec<String> = Vec::new();
    if table_attr {
        collect_table_attribute_provides(root, rel, text, &mut table_attributed, &mut out);
    }
    // Pass 2 — `DbSet<T>` properties (convention naming, minus the same-file overrides above).
    if ef {
        collect_dbset_provides(
            root,
            rel,
            text,
            ef_import,
            false,
            &table_attributed,
            &mut out,
        );
    }
    // Pass 3 — fluent `ToTable("…")` inside `IEntityTypeConfiguration<T>`. It runs BESIDE pass 2 rather
    // than suppressing it: the two shapes live in DIFFERENT FILES in every measured layout, and per-file
    // io projection gives one file no way to silence another's provide. Documented consequence — the
    // convention-named `DbSet` key survives next to the correct fluent one.
    if entity_config {
        entity_config::collect_entity_config_provides(root, rel, text, &mut out);
    }
    out
}

/// True when some class in this file derives from a `…DbContext` base — the FILE-level half of the EF
/// gate, which decides whether either pass runs at all. Per-class attribution is
/// [`class_derives_db_context`]'s job.
fn declares_db_context(node: Node, src: &str) -> bool {
    if node.kind() == "class_declaration" && class_derives_db_context(node, src) {
        return true;
    }
    valid_named_children(node)
        .into_iter()
        .any(|c| declares_db_context(c, src))
}

/// True when THIS class's own base list names a type ending in `DbContext` — the structural half of the
/// EF gate (see the call site for why the import half stopped sufficing).
///
/// `ends_with` rather than an exact match, because deriving from a project's own intermediate context is
/// the norm and each link in that chain still ends in the framework's type name (`IdentityDbContext`,
/// `ApplicationDbContext`). Generic bases (`IdentityDbContext<AppUser>`) and primary-constructor bases
/// (`DbContext(options)`) both arrive as one base entry whose leading token is the type name, so the
/// check reads that token rather than requiring one node kind.
///
/// INTERFACE-shaped names are excluded. C# cannot tell a base class from an implemented interface
/// syntactically, and .NET's own naming guidelines fix the `I` + PascalCase spelling for interfaces — so
/// a `class UnitOfWork : IDbContext` holding `DbSet<T>` properties is a hand-rolled wrapper delegating
/// to a real context, and admitting it would emit that context's tables a second time from the wrong
/// file. That convention is the framework's, not this project's.
pub(super) fn class_derives_db_context(class_node: Node, src: &str) -> bool {
    valid_named_children(class_node)
        .into_iter()
        .filter(|c| c.kind() == "base_list")
        .flat_map(|bases| valid_named_children(bases))
        .filter_map(|b| {
            node_text(b, src)
                .split(['<', ',', '(', ' ', '{'])
                .next()
                .map(str::trim)
        })
        .any(|name| name.ends_with("DbContext") && !is_interface_name(name))
}

/// .NET's interface spelling: `I` followed by an uppercase letter (`IDbContext`, `IDisposable`).
fn is_interface_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next() == Some('I') && chars.next().is_some_and(|c| c.is_ascii_uppercase())
}

// --- [Table] attribute side ---------------------------------------------------------------------------

mod dbset;
mod entity_config;
mod table_attribute;

use dbset::collect_dbset_provides;
use table_attribute::collect_table_attribute_provides;

#[cfg(test)]
mod tests;
