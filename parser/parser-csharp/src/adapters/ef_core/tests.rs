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
