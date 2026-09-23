use super::*;

const EF_USING: &str = "using Microsoft.EntityFrameworkCore;\n";
const SCHEMA_USING: &str = "using System.ComponentModel.DataAnnotations.Schema;\n";

#[test]
fn dbset_property_provides_the_property_named_table() {
    let src = format!(
        "{EF_USING}public class AppDbContext : DbContext {{ public DbSet<User> Users {{ get; set; }} }}\npublic class User {{ public long Id {{ get; set; }} }}"
    );
    let out = extract_ef_core_db_table_provides("Data/AppDbContext.cs", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "table:users");
    assert_eq!(out[0].symbol.as_deref(), Some("User"));
    assert_eq!(out[0].kind, "db-table");
}

#[test]
fn nullable_and_qualified_dbset_shapes_resolve() {
    let src = format!(
        "{EF_USING}public class Ctx : DbContext {{ public DbSet<Models.OrderItem>? OrderItems {{ get; set; }} }}"
    );
    let out = extract_ef_core_db_table_provides("Ctx.cs", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "table:orderItems");
    assert_eq!(out[0].symbol.as_deref(), Some("OrderItem"));
}

#[test]
fn table_attribute_provides_the_literal_name() {
    let src = format!(
        "{SCHEMA_USING}[Table(\"legacy_orders\")]\npublic class Order {{ public long Id {{ get; set; }} }}"
    );
    let out = extract_ef_core_db_table_provides("Models/Order.cs", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "table:legacy_orders");
    assert_eq!(out[0].symbol.as_deref(), Some("Order"));
}

#[test]
fn non_literal_table_attribute_is_skipped_never_guessed() {
    let src = format!(
        "{SCHEMA_USING}[Table(nameof(Order))]\npublic class Order {{ public long Id {{ get; set; }} }}"
    );
    assert!(extract_ef_core_db_table_provides("Order.cs", &src).is_empty());
}

#[test]
fn same_file_table_attribute_suppresses_the_dbset_convention_name() {
    // [Table] overrides EF's DbSet-property naming, so only the attribute-named provide may emit.
    let src = format!(
        "{EF_USING}{SCHEMA_USING}public class Ctx : DbContext {{ public DbSet<Order> Orders {{ get; set; }} }}\n[Table(\"legacy_orders\")]\npublic class Order {{ public long Id {{ get; set; }} }}"
    );
    let out = extract_ef_core_db_table_provides("Ctx.cs", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "table:legacy_orders");
}

#[test]
fn same_file_non_literal_table_attribute_suppresses_without_substitute() {
    // The rename exists but is unreadable — emitting the convention name would key the WRONG table.
    let src = format!(
        "{EF_USING}{SCHEMA_USING}public class Ctx : DbContext {{ public DbSet<Order> Orders {{ get; set; }} }}\n[Table(TableNames.Orders)]\npublic class Order {{ public long Id {{ get; set; }} }}"
    );
    assert!(extract_ef_core_db_table_provides("Ctx.cs", &src).is_empty());
}

#[test]
fn a_generic_property_that_is_not_dbset_is_ignored() {
    let src = format!("{EF_USING}public class Ctx {{ public List<User> Users {{ get; set; }} }}");
    assert!(extract_ef_core_db_table_provides("Ctx.cs", &src).is_empty());
}

#[test]
fn import_gates_block_extraction_without_the_usings() {
    let dbset = "public class Ctx { public DbSet<User> Users { get; set; } }";
    assert!(extract_ef_core_db_table_provides("Ctx.cs", dbset).is_empty());
    let attr = "[Table(\"users\")]\npublic class User { public long Id { get; set; } }";
    assert!(extract_ef_core_db_table_provides("User.cs", attr).is_empty());
}

#[test]
fn each_shape_needs_its_own_gate() {
    // The Schema using alone does not enable DbSet-convention extraction, and vice versa.
    let src =
        format!("{SCHEMA_USING}public class Ctx {{ public DbSet<User> Users {{ get; set; }} }}");
    assert!(extract_ef_core_db_table_provides("Ctx.cs", &src).is_empty());
    let src2 = format!(
        "{EF_USING}[Table(\"users\")]\npublic class User {{ public long Id {{ get; set; }} }}"
    );
    assert!(extract_ef_core_db_table_provides("User.cs", &src2).is_empty());
}

#[test]
fn test_classified_paths_are_silent() {
    // The shared predicate's C#-specific arms (2026-08-03) gate here too: a `tests/` path segment,
    // a C#-conventional `FooTests.cs` name, and a `MyApp.Tests/` project directory are all silent.
    let src = format!(
        "{EF_USING}public class Ctx : DbContext {{ public DbSet<User> Users {{ get; set; }} }}"
    );
    assert!(extract_ef_core_db_table_provides("tests/FixtureContext.cs", &src).is_empty());
    assert!(extract_ef_core_db_table_provides("src/FixtureContextTests.cs", &src).is_empty());
    assert!(extract_ef_core_db_table_provides("Api.Tests/FixtureContext.cs", &src).is_empty());
}

#[test]
fn empty_on_parse_failure() {
    assert!(extract_ef_core_db_table_provides("X.cs", "\u{0}\u{1}not csharp{{{{").is_empty());
}

#[test]
fn a_global_using_context_with_no_import_of_its_own_still_extracts() {
    // C# 10's `global using` moves a project's framework imports into one `GlobalUsings.cs`, so the
    // context file itself declares none — dotnet/eShop's `CatalogContext.cs` opens with
    // `namespace …;` and goes straight to the class. Under an import-only gate its 3 `DbSet<T>`
    // properties, and 9 across the repo's 3 contexts, extracted zero. The file's own base list is the
    // evidence that replaces the import, and EF Core REQUIRES it.
    let src = concat!(
        "namespace eShop.Catalog.API.Infrastructure;\n",
        "public class CatalogContext : DbContext\n",
        "{\n",
        "  public required DbSet<CatalogItem> CatalogItems { get; set; }\n",
        "  public required DbSet<CatalogBrand> CatalogBrands { get; set; }\n",
        "}\n",
    );
    let out =
        extract_ef_core_db_table_provides("src/Catalog.API/Infrastructure/CatalogContext.cs", src);
    let mut keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["table:catalogBrands", "table:catalogItems"],
        "{out:?}"
    );
}

#[test]
fn a_derived_context_gates_but_an_unrelated_base_does_not() {
    // `ends_with` rather than an exact match: deriving from an intermediate context is the norm and
    // every link in that chain still ends in the framework's type name.
    let src = concat!(
        "namespace App;\n",
        "public class AppIdentityDbContext : IdentityDbContext<AppUser>\n",
        "{ public DbSet<Tenant> Tenants { get; set; } }\n",
    );
    let out = extract_ef_core_db_table_provides("App/Ctx.cs", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].key, "table:tenants");

    // The bound in the other direction: a `DbSet<T>`-shaped property on a class that is NOT a context
    // and in a file with no EF import is not evidence of anything, and stays silent.
    let unrelated = concat!(
        "namespace App;\n",
        "public class ViewModel : PageModel\n",
        "{ public DbSet<Tenant> Tenants { get; set; } }\n",
    );
    assert!(
        extract_ef_core_db_table_provides("App/Vm.cs", unrelated).is_empty(),
        "a non-context class with no EF import must not extract"
    );
}

#[test]
fn a_global_using_file_still_honours_a_same_file_table_rename() {
    // The regression the structural gate created and this pin seals. `global using` moves BOTH
    // namespaces into one collector, so widening only the EF gate left pass 1 — the pass that collects
    // the `[Table]` override set — switched off while pass 2 kept emitting. The result was not a
    // missing fact but a WRONG one: `table:users` for an entity the database calls `app_users`, which
    // then keys a phantom provide nothing joins while the real table reads as unprovided.
    let src = concat!(
        "namespace App;\n",
        "[Table(\"app_users\")]\n",
        "public class User { public int Id { get; set; } }\n",
        "public class AppDbContext : DbContext { public DbSet<User> Users { get; set; } }\n",
    );
    let out = extract_ef_core_db_table_provides("App/AppDbContext.cs", src);
    let mut keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["table:app_users"], "{out:?}");
}

#[test]
fn the_structural_gate_belongs_to_one_class_and_not_to_the_file() {
    // Under the IMPORT gate every class in the file contributes, and that stays true. Under the
    // STRUCTURAL gate only the class that actually derives from a context does — otherwise a file
    // holding a context alongside an unrelated type emits that type's `DbSet`-shaped property as if
    // the context declared it.
    let src = concat!(
        "namespace App;\n",
        "public class AppDbContext : DbContext { public DbSet<User> Users { get; set; } }\n",
        "public class Snapshot { public DbSet<Audit> Rows { get; set; } }\n",
    );
    let out = extract_ef_core_db_table_provides("App/Ctx.cs", src);
    let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["table:users"], "{out:?}");

    // With the using present, the file-wide behaviour is byte-for-byte what it always was.
    let with_using = format!("{EF_USING}{src}");
    let out = extract_ef_core_db_table_provides("App/Ctx.cs", &with_using);
    let mut keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["table:rows", "table:users"], "{out:?}");
}

#[test]
fn an_interface_named_like_a_context_is_not_one() {
    // C# cannot tell a base class from an implemented interface syntactically, and .NET's own naming
    // guidelines fix the `I` + PascalCase spelling — so a hand-rolled `IDbContext` wrapper holding
    // `DbSet` properties would otherwise emit the real context's tables a second time, from a file
    // that owns none of them.
    let src = concat!(
        "namespace App;\n",
        "public class UnitOfWork : IDbContext { public DbSet<User> Users { get; set; } }\n",
    );
    assert!(
        extract_ef_core_db_table_provides("App/Uow.cs", src).is_empty(),
        "an interface-shaped base is not evidence of an EF context"
    );
}

/// The per-class narrowing must not reach the import path. Keying it on `class_declaration` alone
/// dropped every `DbSet<T>` declared in an interface — the .NET Clean-Architecture template's own
/// `IApplicationDbContext` shape — because such a property passes through no class on the way down.
#[test]
fn a_dbset_on_an_interface_still_extracts_when_the_using_is_present() {
    let src = format!(
        "{EF_USING}\npublic interface IApplicationDbContext {{ DbSet<TodoList> TodoLists {{ get; }} }}\n"
    );
    let out = extract_ef_core_db_table_provides("src/IApplicationDbContext.cs", &src);
    let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["table:todoLists"], "{out:?}");
}

/// The same for a `record` container, so the fix is pinned as "the import licenses the FILE" rather than
/// as a one-off carve-out for interfaces.
#[test]
fn a_dbset_on_a_record_still_extracts_when_the_using_is_present() {
    let src = format!(
        "{EF_USING}\npublic record Ctx {{ public DbSet<Widget> Widgets {{ get; set; }} }}\n"
    );
    let out = extract_ef_core_db_table_provides("src/Ctx.cs", &src);
    let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["table:widgets"], "{out:?}");
}

/// And the narrowing itself still holds where it is the structural signal doing the work: with no
/// `using`, an unrelated sibling class must not ride the context's gate.
#[test]
fn without_the_using_an_interface_dbset_does_not_ride_a_sibling_context() {
    let src = "public class AppDbContext : DbContext { public DbSet<User> Users { get; set; } }\n\
               public interface ISnapshot { DbSet<Audit> Audits { get; } }\n";
    let out = extract_ef_core_db_table_provides("src/Ctx.cs", src);
    let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["table:users"], "{out:?}");
}

// --- fluent `ToTable` inside `IEntityTypeConfiguration<T>` (pass 3) ---------------------------------

/// The shape eShop actually writes, byte-for-byte: NO `using` line at all (C# 10 `global using`), a
/// non-public class, a base list on its own line, and a receiver named after the entity rather than
/// `builder`. Under the two older gates this file extracted ZERO and the database's real table name
/// (`paymentmethods`) was never seen, while `OrderingContext.cs` emitted `table:payments` for it.
#[test]
fn a_fluent_entity_configuration_provides_the_literal_table_name() {
    let src = concat!(
        "namespace eShop.Ordering.Infrastructure.EntityConfigurations;\n",
        "class PaymentMethodEntityTypeConfiguration\n",
        "    : IEntityTypeConfiguration<PaymentMethod>\n",
        "{\n",
        "    public void Configure(EntityTypeBuilder<PaymentMethod> paymentConfiguration)\n",
        "    {\n",
        "        paymentConfiguration.ToTable(\"paymentmethods\");\n",
        "        paymentConfiguration.Ignore(b => b.DomainEvents);\n",
        "    }\n",
        "}\n",
    );
    let out = extract_ef_core_db_table_provides(
        "src/Ordering.Infrastructure/EntityConfigurations/PaymentMethodEntityTypeConfiguration.cs",
        src,
    );
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].key, "table:paymentmethods");
    assert_eq!(out[0].symbol.as_deref(), Some("PaymentMethod"));
    assert_eq!(out[0].kind, "db-table");
    assert_eq!(
        out[0].line, 7,
        "the ToTable call's own line, not the class's"
    );
}

/// The entity comes from the BASE LIST, never from the receiver — the receiver identifier is an
/// arbitrary parameter name (9 different ones across eShop's 9 configuration files) and reading it
/// would need a type resolver this crate does not have.
#[test]
fn the_entity_comes_from_the_base_list_and_not_from_the_receiver() {
    let src = concat!(
        "namespace App;\n",
        "class CatalogItemEntityTypeConfiguration : IEntityTypeConfiguration<CatalogItem>\n",
        "{ public void Configure(EntityTypeBuilder<CatalogItem> builder) { builder.ToTable(\"Catalog\"); } }\n",
    );
    let out = extract_ef_core_db_table_provides("src/CatalogItemEntityTypeConfiguration.cs", src);
    assert_eq!(out.len(), 1, "{out:?}");
    // `db_table_channel_casing` lowercases the leading character only, so the physical `Catalog`
    // keys as `table:catalog` — NOT as the `DbSet<CatalogItem> CatalogItems` convention name.
    assert_eq!(out[0].key, "table:catalog");
    assert_eq!(out[0].symbol.as_deref(), Some("CatalogItem"));
}

#[test]
fn a_non_literal_to_table_argument_is_skipped_never_guessed() {
    let src = concat!(
        "namespace App;\n",
        "class C : IEntityTypeConfiguration<Order>\n",
        "{ public void Configure(EntityTypeBuilder<Order> b) { b.ToTable(TableNames.Orders); } }\n",
    );
    assert!(extract_ef_core_db_table_provides("src/C.cs", src).is_empty());
}

#[test]
fn a_configuration_class_that_never_calls_to_table_is_silent() {
    // It maps by convention; the `DbSet` arm already names that table and this arm adds nothing.
    let src = concat!(
        "namespace App;\n",
        "class C : IEntityTypeConfiguration<Order>\n",
        "{ public void Configure(EntityTypeBuilder<Order> b) { b.HasKey(o => o.Id); } }\n",
    );
    assert!(extract_ef_core_db_table_provides("src/C.cs", src).is_empty());
}

/// The gate is the base list and nothing else: a bare `ToTable("…")` in a class that implements no
/// configuration interface is not evidence of a mapping, and a `ToTable` on some unrelated builder must
/// not key a table.
#[test]
fn to_table_outside_a_configuration_class_does_not_extract() {
    let src = concat!(
        "namespace App;\n",
        "public static class Ext\n",
        "{ public static void Use(this ModelBuilder b) { b.Entity<Log>(e => { e.ToTable(\"IntegrationEventLog\"); }); } }\n",
    );
    assert!(
        extract_ef_core_db_table_provides("src/Ext.cs", src).is_empty(),
        "the ModelBuilder.Entity<T>(lambda) overload is a separate, deliberately unread shape"
    );
}

/// The gate is also THIS class's own base list — a sibling configuration class in the same file must not
/// lend its gate to a neighbour that implements nothing.
#[test]
fn the_configuration_gate_belongs_to_one_class_and_not_to_the_file() {
    let src = concat!(
        "namespace App;\n",
        "class OrderConfig : IEntityTypeConfiguration<Order>\n",
        "{ public void Configure(EntityTypeBuilder<Order> b) { b.ToTable(\"orders\"); } }\n",
        "class Helper { public void Setup(EntityTypeBuilder<Audit> b) { b.ToTable(\"audits\"); } }\n",
    );
    let out = extract_ef_core_db_table_provides("src/Config.cs", src);
    let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["table:orders"], "{out:?}");
}

/// TWO configuration classes in one file each get their own provide — the walk is per class, not per
/// file, so a file holding several configurations is not collapsed to its first.
#[test]
fn several_configuration_classes_in_one_file_each_provide() {
    let src = concat!(
        "namespace App;\n",
        "class A : IEntityTypeConfiguration<Brand>\n",
        "{ public void Configure(EntityTypeBuilder<Brand> b) { b.ToTable(\"CatalogBrand\"); } }\n",
        "class B : IEntityTypeConfiguration<Kind>\n",
        "{ public void Configure(EntityTypeBuilder<Kind> b) { b.ToTable(\"CatalogType\"); } }\n",
    );
    let out = extract_ef_core_db_table_provides("src/Configs.cs", src);
    let mut keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["table:catalogBrand", "table:catalogType"],
        "{out:?}"
    );
}

/// The documented staged result, pinned so it cannot pass as success by accident: with the `DbSet` and
/// the configuration class in the SAME file both keys emit, and in eShop's real layout (different
/// files) the losing convention key survives the same way. Nothing here suppresses it — that is batch 2.
#[test]
fn the_convention_key_is_not_suppressed_by_the_fluent_one() {
    let src = concat!(
        "namespace App;\n",
        "public class OrderingContext : DbContext { public DbSet<PaymentMethod> Payments { get; set; } }\n",
        "class PaymentConfig : IEntityTypeConfiguration<PaymentMethod>\n",
        "{ public void Configure(EntityTypeBuilder<PaymentMethod> b) { b.ToTable(\"paymentmethods\"); } }\n",
    );
    let out = extract_ef_core_db_table_provides("src/Ctx.cs", src);
    let mut keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["table:paymentmethods", "table:payments"],
        "the fluent arm ADDS the real name; removing the phantom is batch 2 ({out:?})"
    );
}

#[test]
fn a_test_classified_configuration_path_is_silent() {
    let src = concat!(
        "namespace App;\n",
        "class C : IEntityTypeConfiguration<Order>\n",
        "{ public void Configure(EntityTypeBuilder<Order> b) { b.ToTable(\"orders\"); } }\n",
    );
    assert!(extract_ef_core_db_table_provides("Api.Tests/C.cs", src).is_empty());
}

/// Only the FIRST `ToTable` in a class: EF maps one entity per configuration to one table, and a second
/// call belongs to an owned/split entity this arm does not model.
#[test]
fn only_the_first_to_table_in_a_class_provides() {
    let src = concat!(
        "namespace App;\n",
        "class C : IEntityTypeConfiguration<Order>\n",
        "{ public void Configure(EntityTypeBuilder<Order> b) {\n",
        "    b.ToTable(\"orders\");\n",
        "    b.OwnsOne(o => o.Address, a => { a.ToTable(\"order_addresses\"); });\n",
        "} }\n",
    );
    let out = extract_ef_core_db_table_provides("src/C.cs", src);
    let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["table:orders"], "{out:?}");
}
