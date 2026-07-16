//! Coverage for `parse_schema`: model blocks, field declarations/attributes, and
//! @@map/@@unique/@@index block attributes.
use zzop_core::SchemaModel;

use super::*;

const SAMPLE: &str = r#"
// user table
model User {
  id        String   @id @default(uuid(7))
  loginId   String   @unique @map("login_id")
  nickname  String
  createdAt DateTime @default(now()) @map("created_at")

  @@map("users")
}

model Item {
  id      String @id
  ownerId String @map("owner_id")
  name    String
  tags    String[]

  @@unique([ownerId, name])
  @@index([name])
  @@map("items")
}
"#;

fn parse(text: &str) -> Vec<SchemaModel> {
    parse_schema(text, None, None)
}

#[test]
fn extracts_model_names() {
    let models = parse(SAMPLE);
    assert_eq!(
        models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        vec!["User", "Item"]
    );
}

#[test]
fn parses_field_type_and_list_flag() {
    let models = parse(SAMPLE);
    let item = models.iter().find(|m| m.name == "Item").unwrap();
    let tags = item.fields.iter().find(|f| f.name == "tags").unwrap();
    assert_eq!(tags.r#type, "String");
    assert!(tags.list);
}

#[test]
fn parses_field_attributes() {
    let models = parse(SAMPLE);
    let user = models.iter().find(|m| m.name == "User").unwrap();
    let login = user.fields.iter().find(|f| f.name == "loginId").unwrap();
    assert!(login.attrs.iter().any(|a| a.name == "unique"));
    assert_eq!(
        login
            .attrs
            .iter()
            .find(|a| a.name == "map")
            .and_then(|a| a.args.as_deref()),
        Some(r#""login_id""#)
    );
}

#[test]
fn parses_block_attributes() {
    let models = parse(SAMPLE);
    let item = models.iter().find(|m| m.name == "Item").unwrap();
    assert_eq!(item.table_name.as_deref(), Some("items"));
    assert_eq!(
        item.uniques,
        vec![vec!["ownerId".to_string(), "name".to_string()]]
    );
    assert_eq!(item.indexes, vec![vec!["name".to_string()]]);
}

#[test]
fn ignores_comment_lines() {
    let models = parse("// comment\nmodel A { id String @id }\n");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].fields.len(), 1);
}

#[test]
fn parse_then_analyze_vertical_slice() {
    // Vertical slice: schema.prisma -> parse_schema -> analyze_schema -> issues.
    use zzop_rules_schema::analyze_schema;
    let analysis = analyze_schema(parse(SAMPLE));
    // Item.ownerId is an implicit FK (no @relation).
    assert!(analysis
        .issues
        .iter()
        .any(|i| i.rule == "implicit-fk" && i.field.as_deref() == Some("ownerId")));
    // User has createdAt but no updatedAt.
    assert!(analysis
        .issues
        .iter()
        .any(|i| i.rule == "missing-timestamps" && i.model == "User"));
}

// --- parseSchemaEnums ---

const ENUM_SAMPLE: &str = r#"
enum Role {
  USER
  ADMIN
  // internal-only, not yet exposed
  SUPPORT
}

model User {
  id   String @id
  role Role   @default(USER)
}

enum Status {
  ACTIVE @map("active")
  ARCHIVED
}

// @@map on an enum only renames its DB-level type, not its members.
enum Priority {
  LOW MEDIUM HIGH

  @@map("priority_level")
}
"#;

#[test]
fn parse_schema_enums_extracts_enum_names_in_order() {
    let enums = parse_schema_enums(ENUM_SAMPLE);
    assert_eq!(
        enums.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["Role", "Status", "Priority"]
    );
}

#[test]
fn parse_schema_enums_extracts_members_in_declaration_order() {
    let enums = parse_schema_enums(ENUM_SAMPLE);
    let role = enums.iter().find(|e| e.name == "Role").unwrap();
    assert_eq!(role.members, vec!["USER", "ADMIN", "SUPPORT"]);
}

#[test]
fn parse_schema_enums_is_comment_tolerant() {
    // Role's SUPPORT member is preceded by a `//` comment line — must not be dropped or
    // mistaken for a member.
    let enums = parse_schema_enums(ENUM_SAMPLE);
    let role = enums.iter().find(|e| e.name == "Role").unwrap();
    assert!(role.members.contains(&"SUPPORT".to_string()));
    assert_eq!(role.members.len(), 3);
}

#[test]
fn parse_schema_enums_ignores_member_level_map_attribute_text() {
    // "ACTIVE @map(\"active\")" must yield the member "ACTIVE" only, not a second bogus member
    // parsed out of the attribute text.
    let enums = parse_schema_enums(ENUM_SAMPLE);
    let status = enums.iter().find(|e| e.name == "Status").unwrap();
    assert_eq!(status.members, vec!["ACTIVE", "ARCHIVED"]);
}

#[test]
fn parse_schema_enums_ignores_block_map_attribute_line() {
    // Priority's `@@map("priority_level")` line must not be read as members, and the
    // single-line multi-member form must still tokenize correctly.
    let enums = parse_schema_enums(ENUM_SAMPLE);
    let priority = enums.iter().find(|e| e.name == "Priority").unwrap();
    assert_eq!(priority.members, vec!["LOW", "MEDIUM", "HIGH"]);
}

#[test]
fn parse_schema_enums_records_declaration_line() {
    let enums = parse_schema_enums(ENUM_SAMPLE);
    let role = enums.iter().find(|e| e.name == "Role").unwrap();
    // `enum Role {` sits on line 2 of ENUM_SAMPLE (leading newline is line 1).
    assert_eq!(role.line, 2);
}

#[test]
fn parse_schema_enums_returns_empty_for_schema_with_no_enum() {
    assert!(parse_schema_enums(SAMPLE).is_empty());
}

#[test]
fn parse_schema_enums_does_not_affect_model_parsing() {
    // A schema mixing model + enum blocks parses both independently: `parse_schema` still sees
    // only the model.
    let models = parse(ENUM_SAMPLE);
    assert_eq!(
        models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        vec!["User"]
    );
}
