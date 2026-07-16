//! Unit tests for the usage-evidence collectors, the usage-aware cross-check, the churn rule, and
//! `analyze_schema_with_usage`'s composition of structural + usage signals.
use super::*;
use zzop_core::{Attribute, EntityRef, FieldAttr, SchemaField};

// --- fieldUsageTokens ---

#[test]
fn field_usage_tokens_collects_identifiers_from_one_file() {
    let result = field_usage_tokens(
        "src/domains/post/routes/createPostHandlers.ts",
        "export function getPostTitle(post: any) {\n  return post.title;\n}\n",
    );
    assert!(result.contains("title"));
    assert!(result.contains("post"));
}

#[test]
fn field_usage_tokens_dead_field_absent_when_never_referenced() {
    let result = field_usage_tokens(
        "src/domains/post/routes/createPostHandlers.ts",
        "export function f(post: any) { return post.title; }\n",
    );
    assert!(!result.contains("deadField"));
}

#[test]
fn field_usage_tokens_empty_for_a_d_ts_file() {
    let result = field_usage_tokens(
        "src/types/generated.d.ts",
        "export interface Generated { declarationOnlyFieldDEF: string; }\n",
    );
    assert!(result.is_empty());
}

#[test]
fn field_usage_tokens_empty_for_a_js_file() {
    let result = field_usage_tokens(
        "src/domains/post/routes/helper.js",
        "const jsOnlyFieldGHI = 1; module.exports = { jsOnlyFieldGHI };\n",
    );
    assert!(result.is_empty());
}

#[test]
fn field_usage_tokens_excludes_identifiers_inside_comments() {
    let result = field_usage_tokens(
        "src/domains/post/routes/createPostHandlers.ts",
        "// commentOnlyFieldJKL: this is a comment\n/* also commentOnlyFieldJKL */\nexport function f() { return 1; }\n",
    );
    assert!(!result.contains("commentOnlyFieldJKL"));
}

#[test]
fn field_usage_tokens_excludes_identifiers_inside_string_literals() {
    let result = field_usage_tokens(
        "src/domains/post/routes/createPostHandlers.ts",
        "export function f() {\n  const s = \"stringOnlyFieldMNO\";\n  const t = 'stringOnlyFieldMNO';\n  return s + t;\n}\n",
    );
    assert!(!result.contains("stringOnlyFieldMNO"));
}

#[test]
fn field_usage_tokens_tsx_file_also_scanned() {
    let result = field_usage_tokens(
        "src/domains/post/PostCard.tsx",
        "export function PostCard(post: any) { return post.title; }\n",
    );
    assert!(result.contains("title"));
}

// --- crossCheckSchema ---

fn field(name: &str) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        r#type: "String".to_string(),
        optional: false,
        list: false,
        attrs: vec![],
    }
}

fn model(name: &str, field_names: &[&str]) -> SchemaModel {
    SchemaModel {
        name: name.to_string(),
        fields: field_names.iter().map(|n| field(n)).collect(),
        ..Default::default()
    }
}

fn usage(identifiers: &[(&str, u32)]) -> SchemaUsage {
    SchemaUsage {
        identifier_counts: identifiers
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect(),
    }
}

/// An `AttributeStore` asserting a truthy `BOUND_MODEL_ATTR` on each name-only model `Symbol` —
/// the injection replacement for the removed `SchemaUsage.bound_models` set.
fn bound_attrs(names: &[&str]) -> AttributeStore {
    AttributeStore::from_attrs(
        names
            .iter()
            .map(|n| Attribute {
                target: EntityRef::Symbol {
                    name: n.to_string(),
                    file: None,
                },
                key: BOUND_MODEL_ATTR.to_string(),
                value: serde_json::json!(true),
            })
            .collect(),
    )
}

/// An `AttributeStore` carrying `MODEL_CHURN_ATTR` counts per name-only model `Symbol` — the
/// injection replacement for the removed `SchemaUsage.model_churn` map.
fn churn_attrs(pairs: &[(&str, u32)]) -> AttributeStore {
    AttributeStore::from_attrs(
        pairs
            .iter()
            .map(|(n, count)| Attribute {
                target: EntityRef::Symbol {
                    name: n.to_string(),
                    file: None,
                },
                key: MODEL_CHURN_ATTR.to_string(),
                value: serde_json::json!(count),
            })
            .collect(),
    )
}

#[test]
fn cross_check_dead_model_no_store_binding_reported() {
    let issues = cross_check_schema(
        &[model("Orphan", &["id", "payload"])],
        &usage(&[]),
        &AttributeStore::default(),
    );
    assert!(issues
        .iter()
        .any(|i| i.rule == "dead-model" && i.model == "Orphan"));
}

#[test]
fn cross_check_dead_model_bound_model_not_reported() {
    let issues = cross_check_schema(
        &[model("User", &["id", "nickname"])],
        &usage(&[("nickname", 5)]),
        &bound_attrs(&["User"]),
    );
    assert!(!issues.iter().any(|i| i.rule == "dead-model"));
}

#[test]
fn cross_check_dead_field_zero_occurrences_reported() {
    let issues = cross_check_schema(
        &[model("User", &["id", "nickname", "ghostField"])],
        &usage(&[("nickname", 3)]),
        &bound_attrs(&["User"]),
    );
    assert!(issues
        .iter()
        .any(|i| i.rule == "dead-field" && i.field.as_deref() == Some("ghostField")));
    assert!(!issues
        .iter()
        .any(|i| i.rule == "dead-field" && i.field.as_deref() == Some("nickname")));
}

#[test]
fn cross_check_dead_field_excludes_id_created_updated_at() {
    let issues = cross_check_schema(
        &[model("X", &["id", "createdAt", "updatedAt", "name"])],
        &usage(&[]),
        &bound_attrs(&["X"]),
    );
    let dead_fields: Vec<&str> = issues
        .iter()
        .filter(|i| i.rule == "dead-field")
        .map(|i| i.field.as_deref().unwrap())
        .collect();
    assert_eq!(dead_fields, vec!["name"]);
}

#[test]
fn cross_check_dead_field_excludes_short_names() {
    let issues = cross_check_schema(
        &[model("Y", &["id", "ab", "name"])],
        &usage(&[]),
        &bound_attrs(&["Y"]),
    );
    assert!(!issues
        .iter()
        .any(|i| i.rule == "dead-field" && i.field.as_deref() == Some("ab")));
}

#[test]
fn cross_check_dead_field_not_reported_when_parent_is_dead_model() {
    let issues = cross_check_schema(
        &[model("Q", &["id", "name", "payload"])],
        &usage(&[]),
        &AttributeStore::default(),
    );
    assert_eq!(issues.iter().filter(|i| i.rule == "dead-field").count(), 0);
}

// --- applyChurnRule ---

#[test]
fn churn_rule_at_least_5_is_warning() {
    let issues = apply_churn_rule(&[model("User", &["id"])], &churn_attrs(&[("User", 5)]));
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Warning);
}

#[test]
fn churn_rule_at_least_10_is_critical() {
    let issues = apply_churn_rule(&[model("User", &["id"])], &churn_attrs(&[("User", 12)]));
    assert_eq!(issues[0].severity, Severity::Critical);
}

#[test]
fn churn_rule_at_most_4_no_hit() {
    let issues = apply_churn_rule(&[model("User", &["id"])], &churn_attrs(&[("User", 4)]));
    assert_eq!(issues.len(), 0);
}

#[test]
fn churn_rule_model_absent_from_churn_treated_as_zero() {
    let issues = apply_churn_rule(
        &[model("User", &["id"]), model("Item", &["id"])],
        &churn_attrs(&[("User", 6)]),
    );
    assert_eq!(
        issues.iter().map(|i| i.model.as_str()).collect::<Vec<_>>(),
        vec!["User"]
    );
}

#[test]
fn churn_rule_empty_churn_map_no_issues() {
    let issues = apply_churn_rule(&[model("User", &["id"])], &AttributeStore::default());
    assert_eq!(issues.len(), 0);
}

// --- analyzeSchema (usage branch) ---

fn risk_field(name: &str, optional: bool) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        r#type: "String".to_string(),
        optional,
        list: false,
        attrs: if name == "id" {
            vec![FieldAttr {
                name: "id".to_string(),
                args: None,
            }]
        } else {
            vec![]
        },
    }
}

fn risk_model(name: &str, field_names: &[&str]) -> SchemaModel {
    SchemaModel {
        name: name.to_string(),
        fields: field_names.iter().map(|n| risk_field(n, false)).collect(),
        ..Default::default()
    }
}

#[test]
fn analyze_with_usage_structural_only_model_risk_matches_summed_points() {
    let analysis = analyze_schema_with_usage(
        vec![risk_model("P", &["id", "userId", "content"])],
        None,
        &AttributeStore::default(),
    );
    assert!(analysis.model_risk["P"] > 0);
    let expected: i64 = analysis
        .issues
        .iter()
        .filter(|i| i.model == "P")
        .map(|i| severity_points(i.severity))
        .sum();
    assert_eq!(analysis.model_risk["P"], expected);
}

#[test]
fn analyze_with_usage_every_model_gets_model_risk_entry_even_zero_issues() {
    let analysis = analyze_schema_with_usage(
        vec![risk_model("Lookup", &["id", "code"])],
        None,
        &AttributeStore::default(),
    );
    assert_eq!(analysis.model_risk["Lookup"], 0);
}

#[test]
fn analyze_with_usage_signals_add_dead_model_field_and_churn_issues() {
    let analysis = analyze_schema_with_usage(
        vec![risk_model("Ghost", &["id", "secretField"])],
        Some(SchemaUsage::default()),
        &churn_attrs(&[("Ghost", 12)]),
    );
    // Ghost is unbound -> dead-model; churn 12 -> schema-churn critical. dead-field is skipped under dead-model.
    assert!(analysis.issues.iter().any(|i| i.rule == "dead-model"));
    assert!(analysis
        .issues
        .iter()
        .any(|i| i.rule == "schema-churn" && i.severity == Severity::Critical));
}

#[test]
fn analyze_with_usage_no_usage_runs_only_structural_rules() {
    let analysis = analyze_schema_with_usage(
        vec![risk_model("Orphan", &["id", "payload"])],
        None,
        &AttributeStore::default(),
    );
    assert!(!analysis.issues.iter().any(|i| i.rule == "dead-model"));
}
