//! Unit tests for the structural schema rules — moved verbatim from `structural.rs`.
use super::*;
use zzop_core::{FieldAttr, SchemaField};

fn field(
    name: &str,
    ty: &str,
    optional: bool,
    list: bool,
    attrs: &[(&str, Option<&str>)],
) -> SchemaField {
    SchemaField {
        name: name.into(),
        r#type: ty.into(),
        optional,
        list,
        attrs: attrs
            .iter()
            .map(|(n, a)| FieldAttr {
                name: n.to_string(),
                args: a.map(str::to_string),
            })
            .collect(),
    }
}
fn f(name: &str) -> SchemaField {
    field(name, "String", false, false, &[])
}
fn id() -> SchemaField {
    field("id", "String", false, false, &[("id", None)])
}
fn model(
    name: &str,
    fields: Vec<SchemaField>,
    uniques: Vec<Vec<String>>,
    indexes: Vec<Vec<String>>,
) -> SchemaModel {
    SchemaModel {
        name: name.into(),
        fields,
        uniques,
        indexes,
        ..Default::default()
    }
}
fn cols(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}
fn has(issues: &[SchemaIssue], rule: &str, field: &str) -> bool {
    issues
        .iter()
        .any(|i| i.rule == rule && i.field.as_deref() == Some(field))
}

#[test]
fn fk_no_index_hit() {
    let issues = apply_schema_rules(&[model(
        "Post",
        vec![id(), f("ownerId"), f("title")],
        vec![],
        vec![],
    )]);
    assert!(has(&issues, "fk-no-index", "ownerId"));
}

#[test]
fn fk_no_index_covered_by_unique() {
    let issues = apply_schema_rules(&[model(
        "Share",
        vec![id(), f("itemId")],
        vec![cols(&["itemId"])],
        vec![],
    )]);
    assert!(!has(&issues, "fk-no-index", "itemId"));
}

#[test]
fn fk_no_index_leading_composite_member_fully_covered() {
    let issues = apply_schema_rules(&[model(
        "ItemUser",
        vec![id(), f("itemId"), f("userId")],
        vec![cols(&["itemId", "userId"])],
        vec![],
    )]);
    assert!(!has(&issues, "fk-no-index", "itemId"));
}

#[test]
fn fk_no_index_non_leading_composite_member_gets_info_variant() {
    let issues = apply_schema_rules(&[model(
        "Member",
        vec![id(), f("a"), f("guildId")],
        vec![cols(&["a", "guildId"])],
        vec![],
    )]);
    let matches: Vec<_> = issues
        .iter()
        .filter(|i| i.rule == "fk-no-index" && i.field.as_deref() == Some("guildId"))
        .collect();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].severity, Severity::Info);
    let params = matches[0].params.as_ref().unwrap();
    assert_eq!(params["coverage"], "non-leading");
    let composite_cols = params["compositeCols"].as_array().unwrap();
    assert_eq!(
        composite_cols,
        &vec!["a".to_string(), "guildId".to_string()]
    );
}

#[test]
fn missing_timestamps_domain_named_creation_field_with_default_now_satisfies_created_at() {
    let issues = apply_schema_rules(&[model(
        "Event",
        vec![
            id(),
            f("userId"),
            f("payload"),
            field(
                "receivedAt",
                "DateTime",
                false,
                false,
                &[("default", Some("now()"))],
            ),
        ],
        vec![],
        vec![],
    )]);
    let t = issues
        .iter()
        .find(|i| i.rule == "missing-timestamps")
        .expect("expected a missing-timestamps issue for missing updatedAt");
    let missing = t.params.as_ref().unwrap()["missing"].as_array().unwrap();
    assert!(missing.iter().any(|v| v == "updatedAt"));
    assert!(!missing.iter().any(|v| v == "createdAt"));
}

#[test]
fn missing_timestamps_domain_named_creation_field_and_updated_at_present_has_no_issue() {
    let issues = apply_schema_rules(&[model(
        "Event",
        vec![
            id(),
            f("userId"),
            f("payload"),
            field(
                "receivedAt",
                "DateTime",
                false,
                false,
                &[("default", Some("now()"))],
            ),
            field("updatedAt", "DateTime", false, false, &[]),
        ],
        vec![],
        vec![],
    )]);
    assert!(!issues.iter().any(|i| i.rule == "missing-timestamps"));
}

#[test]
fn missing_timestamps_reports_updated_at() {
    let issues = apply_schema_rules(&[model(
        "Log",
        vec![
            id(),
            f("userId"),
            f("msg"),
            field("createdAt", "DateTime", false, false, &[]),
        ],
        vec![],
        vec![],
    )]);
    let t = issues
        .iter()
        .find(|i| i.rule == "missing-timestamps")
        .unwrap();
    let missing = t.params.as_ref().unwrap()["missing"].as_array().unwrap();
    assert!(missing.iter().any(|v| v == "updatedAt"));
}

#[test]
fn missing_timestamps_excludes_lookup() {
    let issues = apply_schema_rules(&[model("Lookup", vec![id(), f("code")], vec![], vec![])]);
    assert!(!issues.iter().any(|i| i.rule == "missing-timestamps"));
}

#[test]
fn god_model_hit() {
    let mut fields = vec![id()];
    for i in 0..16 {
        fields.push(f(&format!("f{i}")));
    }
    let issues = apply_schema_rules(&[model("Big", fields, vec![], vec![])]);
    assert!(issues.iter().any(|i| i.rule == "god-model"));
}

#[test]
fn nullable_fk_hit() {
    let issues = apply_schema_rules(&[model(
        "Item",
        vec![
            id(),
            field("ownerId", "String", true, false, &[]),
            f("name"),
        ],
        vec![],
        vec![],
    )]);
    assert!(has(&issues, "nullable-fk", "ownerId"));
}

#[test]
fn implicit_fk_hit() {
    let issues = apply_schema_rules(&[model(
        "Ref",
        vec![id(), f("userId"), f("name")],
        vec![],
        vec![cols(&["userId"])],
    )]);
    assert!(has(&issues, "implicit-fk", "userId"));
}

#[test]
fn implicit_fk_with_relation_no_hit() {
    let issues = apply_schema_rules(&[model(
        "X",
        vec![
            id(),
            f("targetId"),
            field(
                "target",
                "Target",
                false,
                false,
                &[("relation", Some("fields: [targetId], references: [id]"))],
            ),
        ],
        vec![],
        vec![],
    )]);
    assert!(!issues.iter().any(|i| i.rule == "implicit-fk"));
}

#[test]
fn float_money_hit() {
    let issues = apply_schema_rules(&[model(
        "Order",
        vec![id(), field("totalAmount", "Float", false, false, &[])],
        vec![],
        vec![],
    )]);
    assert!(has(&issues, "float-money", "totalAmount"));
}

#[test]
fn temporal_as_string_hit() {
    let issues = apply_schema_rules(&[model(
        "Ev",
        vec![id(), field("startTime", "String", false, false, &[])],
        vec![],
        vec![],
    )]);
    assert!(has(&issues, "temporal-as-string", "startTime"));
}

#[test]
fn analyze_schema_sums_model_risk() {
    // one warning (nullable-fk) = 2 points on model "Item".
    let a = analyze_schema(vec![model(
        "Item",
        vec![id(), field("ownerId", "String", true, false, &[])],
        vec![cols(&["ownerId"])], // covered -> no fk-no-index; nullable-fk still fires
        vec![],
    )]);
    assert!(a.model_risk["Item"] >= 2);
}
