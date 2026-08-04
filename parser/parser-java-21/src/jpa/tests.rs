use super::*;

const IMPORTS: &str = "import jakarta.persistence.Entity;\nimport jakarta.persistence.Table;\n";

#[test]
fn entity_with_table_name_literal_is_keyed_verbatim() {
    let src =
        format!("{IMPORTS}@Entity\n@Table(name = \"users\")\nclass UserAccount {{ long id; }}");
    let out = extract_jpa_db_table_provides("src/main/java/UserAccount.java", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "table:users");
    assert_eq!(out[0].symbol.as_deref(), Some("UserAccount"));
    assert_eq!(out[0].kind, "db-table");
}

#[test]
fn entity_without_table_defaults_to_snake_cased_class_name() {
    let src = format!("{IMPORTS}@Entity\nclass OrderItem {{ long id; }}");
    let out = extract_jpa_db_table_provides("OrderItem.java", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "table:order_item");
    assert_eq!(out[0].symbol.as_deref(), Some("OrderItem"));
}

#[test]
fn entity_name_attribute_literal_drives_the_default() {
    let src = format!("{IMPORTS}@Entity(name = \"Login\")\nclass UserSession {{ long id; }}");
    let out = extract_jpa_db_table_provides("UserSession.java", &src);
    assert_eq!(out[0].key, "table:login");
}

#[test]
fn table_with_non_literal_name_is_skipped_never_guessed() {
    // Falling back to the class-name default here would key a table the mapping explicitly renames.
    let src =
        format!("{IMPORTS}@Entity\n@Table(name = Names.USERS)\nclass UserAccount {{ long id; }}");
    assert!(extract_jpa_db_table_provides("UserAccount.java", &src).is_empty());
}

#[test]
fn table_with_only_non_name_attributes_still_defaults() {
    let src =
        format!("{IMPORTS}@Entity\n@Table(schema = \"audit\")\nclass AuditLog {{ long id; }}");
    let out = extract_jpa_db_table_provides("AuditLog.java", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "table:audit_log");
}

#[test]
fn table_without_entity_is_not_a_mapping() {
    let src = format!("{IMPORTS}@Table(name = \"users\")\nclass NotAnEntity {{ long id; }}");
    assert!(extract_jpa_db_table_provides("NotAnEntity.java", &src).is_empty());
}

#[test]
fn javax_persistence_import_also_gates() {
    let src = "import javax.persistence.Entity;\n@Entity\nclass LegacyRow { long id; }";
    let out = extract_jpa_db_table_provides("LegacyRow.java", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "table:legacy_row");
}

#[test]
fn glob_import_also_gates() {
    let src = "import jakarta.persistence.*;\n@Entity\nclass GlobRow { long id; }";
    assert_eq!(extract_jpa_db_table_provides("GlobRow.java", src).len(), 1);
}

#[test]
fn import_gate_blocks_a_foreign_entity_annotation() {
    // Some other framework's @Entity (no JPA import anywhere) must not mint a table.
    let src = "import com.example.orm.Entity;\n@Entity\nclass Foreign { long id; }";
    assert!(extract_jpa_db_table_provides("Foreign.java", src).is_empty());
}

#[test]
fn test_classified_paths_are_silent() {
    let src = format!("{IMPORTS}@Entity\nclass Fixture {{ long id; }}");
    assert!(extract_jpa_db_table_provides("src/test/java/com/acme/Fixture.java", &src).is_empty());
    assert!(extract_jpa_db_table_provides("src/main/java/FixtureTest.java", &src).is_empty());
}

#[test]
fn empty_on_parse_failure() {
    assert!(extract_jpa_db_table_provides("X.java", "\u{0}\u{1}not java{{{{").is_empty());
}

/// The DIVERGENCE set between Hibernate's real `CamelCaseToUnderscoresNamingStrategy` (the
/// lower→Upper→lower triple rule this crate now reproduces) and the earlier "underscore before any
/// uppercase after a lowercase/digit" approximation, which produced `order_id`/`user_dto`/
/// `product_sku`/`line1_item`/`user_a` for these — names Hibernate never keys, so the join missed the
/// real table on every one of them.
#[test]
fn camel_to_snake_matches_hibernates_actual_algorithm_on_divergent_shapes() {
    assert_eq!(camel_to_snake("OrderID"), "orderid");
    assert_eq!(camel_to_snake("UserDTO"), "userdto");
    assert_eq!(camel_to_snake("ProductSKU"), "productsku");
    assert_eq!(camel_to_snake("Line1Item"), "line1item");
    assert_eq!(camel_to_snake("UserA"), "usera");
}

/// The AGREEMENT set: shapes where the old approximation and Hibernate's algorithm coincide must not
/// move (the engine integration test `analyze_java_cross_layer` pins `table:order_item` end-to-end).
#[test]
fn camel_to_snake_agreement_shapes_are_unchanged() {
    assert_eq!(camel_to_snake("OrderItem"), "order_item");
    assert_eq!(camel_to_snake("UserAccount"), "user_account");
    assert_eq!(camel_to_snake("HTTPLog"), "httplog");
}
