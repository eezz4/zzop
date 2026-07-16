//! Rule bodies and index-coverage helpers for the structural schema rules — moved verbatim from
//! `structural.rs` (the thin root keeps `STRUCTURAL_RULES_VERSION`, the census-pinned consts, the
//! IR-facing types, and the `analyze_schema`/`apply_schema_rules` orchestrators).

use zzop_core::{SchemaField, SchemaModel, Severity};

use super::{SchemaIssue, GOD_THRESHOLD, LOOKUP_FIELD_MAX, MONEY_TOKENS};

fn issue(rule: &str, severity: Severity, model: &str, field: Option<&str>) -> SchemaIssue {
    SchemaIssue {
        rule: rule.to_string(),
        severity,
        model: model.to_string(),
        field: field.map(str::to_string),
        params: None,
    }
}

pub(super) fn rule_god_model(model: &SchemaModel, out: &mut Vec<SchemaIssue>) {
    if model.fields.len() < GOD_THRESHOLD {
        return;
    }
    let mut i = issue("god-model", Severity::Warning, &model.name, None);
    i.params = Some(serde_json::json!({ "fieldCount": model.fields.len() }));
    out.push(i);
}

pub(super) fn rule_missing_timestamps(model: &SchemaModel, out: &mut Vec<SchemaIssue>) {
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

pub(super) fn rule_redundant_index(model: &SchemaModel, out: &mut Vec<SchemaIssue>) {
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

pub(super) fn rule_float_money(
    model: &SchemaModel,
    field: &SchemaField,
    out: &mut Vec<SchemaIssue>,
) {
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

pub(super) fn rule_stale_updated_at(
    model: &SchemaModel,
    field: &SchemaField,
    out: &mut Vec<SchemaIssue>,
) {
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

pub(super) fn rule_temporal_as_string(
    model: &SchemaModel,
    field: &SchemaField,
    out: &mut Vec<SchemaIssue>,
) {
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

pub(super) fn rule_fk_no_index(
    model: &SchemaModel,
    field: &SchemaField,
    out: &mut Vec<SchemaIssue>,
) {
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

pub(super) fn rule_nullable_fk(
    model: &SchemaModel,
    field: &SchemaField,
    out: &mut Vec<SchemaIssue>,
) {
    if field.optional {
        out.push(issue(
            "nullable-fk",
            Severity::Warning,
            &model.name,
            Some(&field.name),
        ));
    }
}

pub(super) fn rule_implicit_fk(
    model: &SchemaModel,
    field: &SchemaField,
    out: &mut Vec<SchemaIssue>,
) {
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

pub(super) fn is_fk_candidate(field: &SchemaField) -> bool {
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
