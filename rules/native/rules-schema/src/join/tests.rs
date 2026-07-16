//! Unit tests for the schema x usage JOIN rules — moved verbatim from `join.rs` (tests are
//! exempt from the 300-line source cap and live as a sibling module).
use super::*;
use zzop_core::{FieldAttr, SchemaField};

fn field(name: &str, ty: &str, attrs: &[&str]) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        r#type: ty.to_string(),
        optional: false,
        list: false,
        attrs: attrs
            .iter()
            .map(|a| FieldAttr {
                name: a.to_string(),
                args: None,
            })
            .collect(),
    }
}

fn model(
    name: &str,
    fields: Vec<SchemaField>,
    uniques: Vec<Vec<String>>,
    indexes: Vec<Vec<String>>,
) -> SchemaModel {
    SchemaModel {
        name: name.to_string(),
        fields,
        uniques,
        indexes,
        ..Default::default()
    }
}

fn cols(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

// --- softDeleteBypassIssues ---

#[test]
fn soft_delete_bypass_hits_when_filter_absent() {
    let models = vec![model(
        "Item",
        vec![
            field("id", "String", &["id"]),
            field("deletedAt", "DateTime", &[]),
        ],
        vec![],
        vec![],
    )];
    let sites = vec![QueryCallSite {
        model: "Item".to_string(),
        method: "findMany".to_string(),
        file: "a.ts".to_string(),
        line: 5,
        call_text: "({ where: { ownerId: 1 } })".to_string(),
    }];
    let issues = soft_delete_bypass_issues(&models, &sites);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule, "soft-delete-bypass");
    assert_eq!(issues[0].field.as_deref(), Some("deletedAt"));
    assert_eq!(issues[0].line, 5);
}

#[test]
fn soft_delete_bypass_no_hit_when_filter_present() {
    let models = vec![model(
        "Item",
        vec![
            field("id", "String", &["id"]),
            field("deletedAt", "DateTime", &[]),
        ],
        vec![],
        vec![],
    )];
    let sites = vec![QueryCallSite {
        model: "Item".to_string(),
        method: "findMany".to_string(),
        file: "a.ts".to_string(),
        line: 5,
        call_text: "({ where: { deletedAt: null } })".to_string(),
    }];
    assert!(soft_delete_bypass_issues(&models, &sites).is_empty());
}

#[test]
fn soft_delete_bypass_no_hit_when_model_has_no_soft_delete_field() {
    let models = vec![model(
        "Item",
        vec![field("id", "String", &["id"])],
        vec![],
        vec![],
    )];
    let sites = vec![QueryCallSite {
        model: "Item".to_string(),
        method: "findMany".to_string(),
        file: "a.ts".to_string(),
        line: 5,
        call_text: "({})".to_string(),
    }];
    assert!(soft_delete_bypass_issues(&models, &sites).is_empty());
}

#[test]
fn soft_delete_bypass_snake_case_variant_also_recognized() {
    let models = vec![model(
        "Item",
        vec![
            field("id", "String", &["id"]),
            field("deleted_at", "DateTime", &[]),
        ],
        vec![],
        vec![],
    )];
    let sites = vec![QueryCallSite {
        model: "Item".to_string(),
        method: "count".to_string(),
        file: "a.ts".to_string(),
        line: 1,
        call_text: "({})".to_string(),
    }];
    let issues = soft_delete_bypass_issues(&models, &sites);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].field.as_deref(), Some("deleted_at"));
}

#[test]
fn soft_delete_bypass_ignores_sites_on_other_models() {
    let models = vec![model(
        "Item",
        vec![
            field("id", "String", &["id"]),
            field("deletedAt", "DateTime", &[]),
        ],
        vec![],
        vec![],
    )];
    let sites = vec![QueryCallSite {
        model: "Other".to_string(),
        method: "findMany".to_string(),
        file: "a.ts".to_string(),
        line: 1,
        call_text: "({})".to_string(),
    }];
    assert!(soft_delete_bypass_issues(&models, &sites).is_empty());
}

// --- orderbyUnindexedIssues ---

fn site(model: &str, call_text: &str) -> QueryCallSite {
    QueryCallSite {
        model: model.to_string(),
        method: "findMany".to_string(),
        file: "a.ts".to_string(),
        line: 3,
        call_text: call_text.to_string(),
    }
}

#[test]
fn orderby_unindexed_hits_when_field_has_no_coverage() {
    let models = vec![model(
        "Item",
        vec![field("id", "String", &["id"]), field("name", "String", &[])],
        vec![],
        vec![],
    )];
    let sites = vec![site("Item", "({ orderBy: { name: 'asc' } })")];
    let issues = orderby_unindexed_issues(&models, &sites);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule, "orderby-unindexed");
    assert_eq!(issues[0].field.as_deref(), Some("name"));
}

#[test]
fn orderby_unindexed_no_hit_when_field_is_id() {
    let models = vec![model(
        "Item",
        vec![field("id", "String", &["id"])],
        vec![],
        vec![],
    )];
    let sites = vec![site("Item", "({ orderBy: { id: 'asc' } })")];
    assert!(orderby_unindexed_issues(&models, &sites).is_empty());
}

#[test]
fn orderby_unindexed_no_hit_when_field_is_unique() {
    let models = vec![model(
        "Item",
        vec![
            field("id", "String", &["id"]),
            field("slug", "String", &["unique"]),
        ],
        vec![],
        vec![],
    )];
    let sites = vec![site("Item", "({ orderBy: { slug: 'desc' } })")];
    assert!(orderby_unindexed_issues(&models, &sites).is_empty());
}

#[test]
fn orderby_unindexed_no_hit_when_field_is_leading_index_column() {
    let models = vec![model(
        "Item",
        vec![
            field("id", "String", &["id"]),
            field("status", "String", &[]),
            field("createdAt", "DateTime", &[]),
        ],
        vec![],
        vec![cols(&["status", "createdAt"])],
    )];
    let sites = vec![site("Item", "({ orderBy: { status: 'asc' } })")];
    assert!(orderby_unindexed_issues(&models, &sites).is_empty());
}

#[test]
fn orderby_unindexed_hits_when_field_is_trailing_index_column_only() {
    let models = vec![model(
        "Item",
        vec![
            field("id", "String", &["id"]),
            field("status", "String", &[]),
            field("createdAt", "DateTime", &[]),
        ],
        vec![],
        vec![cols(&["status", "createdAt"])],
    )];
    let sites = vec![site("Item", "({ orderBy: { createdAt: 'asc' } })")];
    let issues = orderby_unindexed_issues(&models, &sites);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].field.as_deref(), Some("createdAt"));
}

#[test]
fn orderby_unindexed_no_hit_when_field_is_leading_unique_column() {
    let models = vec![model(
        "Item",
        vec![
            field("id", "String", &["id"]),
            field("ownerId", "String", &[]),
            field("name", "String", &[]),
        ],
        vec![cols(&["ownerId", "name"])],
        vec![],
    )];
    let sites = vec![site("Item", "({ orderBy: { ownerId: 'asc' } })")];
    assert!(orderby_unindexed_issues(&models, &sites).is_empty());
}

#[test]
fn orderby_unindexed_skips_multi_field_order_by_object() {
    let models = vec![model(
        "Item",
        vec![field("id", "String", &["id"]), field("name", "String", &[])],
        vec![],
        vec![],
    )];
    let sites = vec![site(
        "Item",
        "({ orderBy: { name: 'asc', createdAt: 'desc' } })",
    )];
    assert!(orderby_unindexed_issues(&models, &sites).is_empty());
}

#[test]
fn orderby_unindexed_skips_array_order_by() {
    let models = vec![model(
        "Item",
        vec![field("id", "String", &["id"]), field("name", "String", &[])],
        vec![],
        vec![],
    )];
    let sites = vec![site("Item", "({ orderBy: [{ name: 'asc' }] })")];
    assert!(orderby_unindexed_issues(&models, &sites).is_empty());
}

#[test]
fn orderby_unindexed_skips_unresolvable_field_name() {
    let models = vec![model(
        "Item",
        vec![field("id", "String", &["id"])],
        vec![],
        vec![],
    )];
    let sites = vec![site("Item", "({ orderBy: { ghost: 'asc' } })")];
    assert!(orderby_unindexed_issues(&models, &sites).is_empty());
}

#[test]
fn orderby_unindexed_skips_when_model_unresolvable() {
    let models: Vec<SchemaModel> = vec![];
    let sites = vec![site("Item", "({ orderBy: { name: 'asc' } })")];
    assert!(orderby_unindexed_issues(&models, &sites).is_empty());
}

#[test]
fn orderby_unindexed_skips_when_no_order_by_present() {
    let models = vec![model(
        "Item",
        vec![field("id", "String", &["id"]), field("name", "String", &[])],
        vec![],
        vec![],
    )];
    let sites = vec![site("Item", "({ where: { name: 'x' } })")];
    assert!(orderby_unindexed_issues(&models, &sites).is_empty());
}

// --- enumStringDriftIssues ---

fn schema_enum(name: &str, members: &[&str]) -> SchemaEnum {
    SchemaEnum {
        name: name.to_string(),
        members: members.iter().map(|m| m.to_string()).collect(),
        line: 1,
    }
}

fn call_site(model: &str, line: u32, call_text: &str) -> QueryCallSite {
    QueryCallSite {
        model: model.to_string(),
        method: "findMany".to_string(),
        file: "a.ts".to_string(),
        line,
        call_text: call_text.to_string(),
    }
}

#[test]
fn enum_string_drift_no_fire_when_literal_is_a_member() {
    let enums = vec![schema_enum("Role", &["USER", "ADMIN"])];
    let models = vec![model(
        "User",
        vec![field("id", "String", &["id"]), field("role", "Role", &[])],
        vec![],
        vec![],
    )];
    let sites = vec![call_site("User", 4, "({ where: { role: 'ADMIN' } })")];
    assert!(enum_string_drift_issues(&models, &enums, &sites).is_empty());
}

#[test]
fn enum_string_drift_fires_when_literal_is_not_a_member() {
    let enums = vec![schema_enum("Role", &["USER", "ADMIN"])];
    let models = vec![model(
        "User",
        vec![field("id", "String", &["id"]), field("role", "Role", &[])],
        vec![],
        vec![],
    )];
    let sites = vec![call_site("User", 4, "({ where: { role: 'ADMNI' } })")];
    let issues = enum_string_drift_issues(&models, &enums, &sites);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule, "enum-string-drift");
    assert_eq!(issues[0].model, "User");
    assert_eq!(issues[0].field.as_deref(), Some("role"));
    assert_eq!(issues[0].line, 4);
    assert_eq!(issues[0].file, "a.ts");
    assert_eq!(issues[0].severity, Severity::Warning);
    assert_eq!(
        issues[0].params.as_ref().unwrap()["literal"].as_str(),
        Some("ADMNI")
    );
    assert_eq!(
        issues[0].params.as_ref().unwrap()["enum"].as_str(),
        Some("Role")
    );
}

#[test]
fn enum_string_drift_skips_ambiguous_field_name_across_models() {
    let enums = vec![
        schema_enum("Role", &["USER", "ADMIN"]),
        schema_enum("Status", &["ACTIVE", "ARCHIVED"]),
    ];
    let models = vec![
        model(
            "User",
            vec![field("id", "String", &["id"]), field("status", "Role", &[])],
            vec![],
            vec![],
        ),
        model(
            "Order",
            vec![
                field("id", "String", &["id"]),
                field("status", "Status", &[]),
            ],
            vec![],
            vec![],
        ),
    ];
    let sites = vec![
        call_site("User", 4, "({ where: { status: 'BOGUS' } })"),
        call_site("Order", 8, "({ where: { status: 'BOGUS' } })"),
    ];
    // "status" maps to Role on User and Status on Order -> ambiguous -> both skipped.
    assert!(enum_string_drift_issues(&models, &enums, &sites).is_empty());
}

#[test]
fn enum_string_drift_no_op_when_schema_has_no_enum() {
    let models = vec![model(
        "User",
        vec![field("id", "String", &["id"]), field("role", "String", &[])],
        vec![],
        vec![],
    )];
    let sites = vec![call_site("User", 4, "({ where: { role: 'ADMIN' } })")];
    assert!(enum_string_drift_issues(&models, &[], &sites).is_empty());
}

#[test]
fn enum_string_drift_skips_field_not_actually_enum_typed_on_this_model() {
    // Guest's own "role" field is a plain String, not the Role enum -- must not be flagged.
    let enums = vec![schema_enum("Role", &["USER", "ADMIN"])];
    let models = vec![
        model(
            "Admin",
            vec![field("id", "String", &["id"]), field("role", "Role", &[])],
            vec![],
            vec![],
        ),
        model(
            "Guest",
            vec![field("id", "String", &["id"]), field("role", "String", &[])],
            vec![],
            vec![],
        ),
    ];
    let sites = vec![call_site("Guest", 4, "({ where: { role: 'anything' } })")];
    assert!(enum_string_drift_issues(&models, &enums, &sites).is_empty());
}

#[test]
fn enum_string_drift_skips_literal_inside_in_array() {
    let enums = vec![schema_enum("Role", &["USER", "ADMIN"])];
    let models = vec![model(
        "User",
        vec![field("id", "String", &["id"]), field("role", "Role", &[])],
        vec![],
        vec![],
    )];
    let sites = vec![call_site(
        "User",
        4,
        "({ where: { role: { in: ['BOGUS'] } } })",
    )];
    assert!(enum_string_drift_issues(&models, &enums, &sites).is_empty());
}

#[test]
fn enum_string_drift_deduplicates_repeated_bad_literal_at_same_site() {
    let enums = vec![schema_enum("Role", &["USER", "ADMIN"])];
    let models = vec![model(
        "User",
        vec![field("id", "String", &["id"]), field("role", "Role", &[])],
        vec![],
        vec![],
    )];
    // Contrived: same bad literal twice in one call span.
    let sites = vec![call_site(
        "User",
        4,
        "({ where: { OR: [{ role: 'BOGUS' }, { role: 'BOGUS' }] } })",
    )];
    let issues = enum_string_drift_issues(&models, &enums, &sites);
    assert_eq!(issues.len(), 1);
}
