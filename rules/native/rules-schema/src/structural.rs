//! Prisma schema structural rules — source-agnostic checks over the schema IR (`zpz_core::schema`).
//! IR types (`SchemaModel` etc.) live in `zpz-core`; the rule bodies that operate on them live here.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use zpz_core::{SchemaField, SchemaModel, Severity};

/// Version token for `apply_schema_rules`'s output shape, folded into the ruleset cache fingerprint so
/// a stale cache doesn't keep serving old `schema/*` findings. Bump when the output shape changes.
pub const STRUCTURAL_RULES_VERSION: &str = "v1";

/// A structural schema issue (source-agnostic; from a single model/field). `camelCase` here matches
/// every other output-facing type, since this struct serializes verbatim into `Finding.data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaIssue {
    pub rule: String,
    pub severity: Severity,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Rule-specific auxiliary parameters (god-model fieldCount, missing-timestamps missing[], ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// Not `#[serde(rename_all = "camelCase")]`: `analyze_schema` is only used by this crate's own tests and
/// never crosses the napi JSON boundary, so `model_risk` stays as declared. Add the attribute if that changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchemaAnalysis {
    pub models: Vec<SchemaModel>,
    pub issues: Vec<SchemaIssue>,
    /// modelName -> risk score (sum of severity points).
    pub model_risk: HashMap<String, i64>,
}

pub(crate) fn severity_points(s: Severity) -> i64 {
    match s {
        Severity::Critical => 5,
        Severity::Warning => 2,
        Severity::Info => 1,
    }
}

const GOD_THRESHOLD: usize = 15;
/// Models with at most this many fields are excluded from the timestamps rule (assumed lookup tables).
const LOOKUP_FIELD_MAX: usize = 3;

/// Field-name tokens denoting a whole monetary amount (matched as a case-insensitive substring).
const MONEY_TOKENS: &[&str] = &[
    "price",
    "amount",
    "cost",
    "total",
    "subtotal",
    "balance",
    "salary",
    "wage",
    "payment",
    "payout",
    "payable",
    "receivable",
    "refund",
    "rebate",
    "fee",
    "fare",
    "tariff",
    "surcharge",
    "deposit",
    "revenue",
    "income",
    "expense",
    "budget",
    "profit",
    "tax",
    "discount",
    "charge",
    "credit",
    "debit",
    "commission",
    "currency",
    "money",
    "cash",
    "invoice",
    "billing",
    "premium",
    "allowance",
    "bonus",
];

/// Analyze schema models -> issues + per-model risk. Structural-only path (usage rules require a code scan).
pub fn analyze_schema(models: Vec<SchemaModel>) -> SchemaAnalysis {
    let issues = apply_schema_rules(&models);
    let mut model_risk: HashMap<String, i64> = models.iter().map(|m| (m.name.clone(), 0)).collect();
    for issue in &issues {
        *model_risk.entry(issue.model.clone()).or_insert(0) += severity_points(issue.severity);
    }
    SchemaAnalysis {
        models,
        issues,
        model_risk,
    }
}

pub fn apply_schema_rules(models: &[SchemaModel]) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    for model in models {
        rule_god_model(model, &mut issues);
        rule_missing_timestamps(model, &mut issues);
        rule_redundant_index(model, &mut issues);
        for field in &model.fields {
            rule_float_money(model, field, &mut issues);
            rule_stale_updated_at(model, field, &mut issues);
            rule_temporal_as_string(model, field, &mut issues);
            if !is_fk_candidate(field) {
                continue;
            }
            rule_fk_no_index(model, field, &mut issues);
            rule_nullable_fk(model, field, &mut issues);
            rule_implicit_fk(model, field, &mut issues);
        }
    }
    issues
}

fn issue(rule: &str, severity: Severity, model: &str, field: Option<&str>) -> SchemaIssue {
    SchemaIssue {
        rule: rule.to_string(),
        severity,
        model: model.to_string(),
        field: field.map(str::to_string),
        params: None,
    }
}

fn rule_god_model(model: &SchemaModel, out: &mut Vec<SchemaIssue>) {
    if model.fields.len() < GOD_THRESHOLD {
        return;
    }
    let mut i = issue("god-model", Severity::Warning, &model.name, None);
    i.params = Some(serde_json::json!({ "fieldCount": model.fields.len() }));
    out.push(i);
}

fn rule_missing_timestamps(model: &SchemaModel, out: &mut Vec<SchemaIssue>) {
    if model.fields.len() <= LOOKUP_FIELD_MAX {
        return;
    }
    let names: std::collections::HashSet<&str> =
        model.fields.iter().map(|f| f.name.as_str()).collect();
    // A creation timestamp is satisfied by a field named `createdAt`, or by any `DateTime
    // @default(now())` field (e.g. `receivedAt` on an append-only event model) — both are equally valid.
    let has_creation_ts = names.contains("createdAt") || model.fields.iter().any(has_default_now);
    let mut missing = Vec::new();
    if !has_creation_ts {
        missing.push("createdAt");
    }
    if !names.contains("updatedAt") {
        missing.push("updatedAt");
    }
    if missing.is_empty() {
        return;
    }
    let mut i = issue("missing-timestamps", Severity::Info, &model.name, None);
    i.params = Some(serde_json::json!({ "missing": missing }));
    out.push(i);
}

fn has_default_now(field: &SchemaField) -> bool {
    field.r#type.eq_ignore_ascii_case("DateTime")
        && field
            .attrs
            .iter()
            .any(|a| a.name == "default" && a.args.as_deref().unwrap_or("").contains("now()"))
}

fn rule_redundant_index(model: &SchemaModel, out: &mut Vec<SchemaIssue>) {
    let single_unique: std::collections::HashSet<&str> = model
        .uniques
        .iter()
        .filter(|g| g.len() == 1)
        .map(|g| g[0].as_str())
        .collect();
    let already_indexed = |col: &str| -> bool {
        if single_unique.contains(col) {
            return true;
        }
        model
            .fields
            .iter()
            .find(|x| x.name == col)
            .is_some_and(|f| has_attr(f, "id") || has_attr(f, "unique"))
    };
    for group in &model.indexes {
        if group.len() != 1 {
            continue;
        }
        let col = group[0].as_str();
        if already_indexed(col) {
            out.push(issue(
                "redundant-index",
                Severity::Info,
                &model.name,
                Some(col),
            ));
        }
    }
}

fn rule_float_money(model: &SchemaModel, field: &SchemaField, out: &mut Vec<SchemaIssue>) {
    let t = field.r#type.to_ascii_lowercase();
    if t != "float" && t != "double" && t != "real" {
        return;
    }
    let lower = field.name.to_ascii_lowercase();
    if !MONEY_TOKENS.iter().any(|tok| lower.contains(tok)) {
        return;
    }
    let mut i = issue(
        "float-money",
        Severity::Warning,
        &model.name,
        Some(&field.name),
    );
    i.params = Some(serde_json::json!({ "type": field.r#type }));
    out.push(i);
}

fn rule_stale_updated_at(model: &SchemaModel, field: &SchemaField, out: &mut Vec<SchemaIssue>) {
    if field.name != "updatedAt" {
        return;
    }
    let t = field.r#type.to_ascii_lowercase();
    if t != "datetime" && t != "timestamp" && t != "date" {
        return;
    }
    if has_attr(field, "updatedAt") {
        return;
    }
    out.push(issue(
        "stale-updated-at",
        Severity::Warning,
        &model.name,
        Some(&field.name),
    ));
}

fn rule_temporal_as_string(model: &SchemaModel, field: &SchemaField, out: &mut Vec<SchemaIssue>) {
    if field.r#type != "String" {
        return; // Int/BigInt epoch is legitimate; only flag text-stored dates.
    }
    let n = &field.name;
    let suffix =
        n.ends_with("At") || n.ends_with("Date") || n.ends_with("Time") || n.ends_with("Timestamp");
    let exact = matches!(
        n.to_ascii_lowercase().as_str(),
        "date" | "time" | "timestamp" | "datetime"
    );
    if suffix || exact {
        out.push(issue(
            "temporal-as-string",
            Severity::Warning,
            &model.name,
            Some(n),
        ));
    }
}

fn rule_fk_no_index(model: &SchemaModel, field: &SchemaField, out: &mut Vec<SchemaIssue>) {
    if has_attr(field, "unique") || has_attr(field, "id") {
        return;
    }
    match index_coverage(&field.name, &model.uniques, &model.indexes) {
        Coverage::Leading => {}
        Coverage::NonLeading { cols, kind } => {
            let mut i = issue(
                "fk-no-index",
                Severity::Info,
                &model.name,
                Some(&field.name),
            );
            i.params = Some(serde_json::json!({
                "coverage": "non-leading",
                "compositeCols": cols,
                "compositeKind": kind,
            }));
            out.push(i);
        }
        Coverage::None => {
            out.push(issue(
                "fk-no-index",
                Severity::Warning,
                &model.name,
                Some(&field.name),
            ));
        }
    }
}

fn rule_nullable_fk(model: &SchemaModel, field: &SchemaField, out: &mut Vec<SchemaIssue>) {
    if field.optional {
        out.push(issue(
            "nullable-fk",
            Severity::Warning,
            &model.name,
            Some(&field.name),
        ));
    }
}

fn rule_implicit_fk(model: &SchemaModel, field: &SchemaField, out: &mut Vec<SchemaIssue>) {
    if has_attr(field, "relation") || has_attr(field, "unique") {
        return;
    }
    let modeled = model.fields.iter().any(|f| {
        f.attrs
            .iter()
            .any(|a| a.name == "relation" && a.args.as_deref().unwrap_or("").contains(&field.name))
    });
    if modeled {
        return;
    }
    out.push(issue(
        "implicit-fk",
        Severity::Info,
        &model.name,
        Some(&field.name),
    ));
}

fn is_fk_candidate(field: &SchemaField) -> bool {
    if field.name == "id" || field.name == "_id" {
        return false;
    }
    if !field.name.ends_with("Id") && !field.name.to_ascii_lowercase().ends_with("_id") {
        return false;
    }
    matches!(field.r#type.as_str(), "String" | "Int" | "BigInt")
}

fn has_attr(field: &SchemaField, name: &str) -> bool {
    field.attrs.iter().any(|a| a.name == name)
}

/// A field's `@@index`/`@@unique` coverage relative to a single-column lookup: `Leading` if the field
/// leads some group (fully covered — composite indexes serve lookups via their leading prefix);
/// `NonLeading` if it appears later in a group but never leads one (covered only for queries that also
/// constrain the leading column(s)); `None` if it never appears in any group.
pub(crate) enum Coverage {
    Leading,
    NonLeading {
        cols: Vec<String>,
        kind: &'static str,
    },
    None,
}

/// Tie-break for a field in multiple groups: `uniques` is checked before `indexes`, first hit wins.
fn index_coverage(field_name: &str, uniques: &[Vec<String>], indexes: &[Vec<String>]) -> Coverage {
    let leads = |groups: &[Vec<String>]| {
        groups
            .iter()
            .any(|g| g.first().map(String::as_str) == Some(field_name))
    };
    if leads(uniques) || leads(indexes) {
        return Coverage::Leading;
    }
    for g in uniques {
        if g.iter().any(|c| c == field_name) {
            return Coverage::NonLeading {
                cols: g.clone(),
                kind: "unique",
            };
        }
    }
    for g in indexes {
        if g.iter().any(|c| c == field_name) {
            return Coverage::NonLeading {
                cols: g.clone(),
                kind: "index",
            };
        }
    }
    Coverage::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use zpz_core::FieldAttr;

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
}
