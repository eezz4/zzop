//! Schema x usage JOIN rules — `soft-delete-bypass`, `orderby-unindexed`, `enum-string-drift`: rules whose
//! verdict needs BOTH the schema IR (`SchemaModel`/`SchemaField`/attrs, `SchemaEnum`) AND a call-site scan
//! of BE source (which model + method a query call targets, and its argument-span text). `usage.rs`'s
//! collectors only produce aggregates with no positional evidence, so these rules instead take a
//! pre-collected `&[zzop_core::QueryCallSite]` that keeps file/line/call-text per call site rather than
//! folding it into a count — produced per-file by `zzop_parser_typescript::extract_query_call_sites`
//! during the fused pass and assembled tree-wide by `zzop_engine::analyze::run_schema_join_rules`.
//!
//! - `soft-delete-bypass`: flags `findMany`/`findFirst`/`findUnique`/`count` call sites on a model with a
//!   `deletedAt`/`deleted_at` field whose argument span never mentions that field name — conservative by
//!   construction, so false negatives are preferred (see `soft_delete_bypass_issues`'s doc for the blind spot).
//! - `orderby-unindexed`: decidable subset only — a single-field `orderBy: { field: 'asc' }` object (not a
//!   multi-key object or the array form used for multi-field ordering) on a resolvable model, where
//!   `field` has no `@id`/`@unique` of its own and is not the leading column of any `@@index`/`@@unique`.
//! - `enum-string-drift`: for a field whose type resolves to a declared enum (via
//!   `zzop_parser_prisma::parse_schema_enums`) and whose field name maps to exactly one enum type across every
//!   model (ambiguous names are skipped), flags direct literal-object `fieldName: 'Literal'` occurrences whose
//!   value isn't a declared enum member; a literal inside `in: [...]`, a variable, or a nested value is skipped.

use std::collections::HashMap;

use regex::Regex;
use serde::{Deserialize, Serialize};

pub use zzop_core::QueryCallSite;
use zzop_core::{SchemaEnum, SchemaModel, Severity};

/// A schema x usage JOIN issue. Unlike `structural::SchemaIssue` (anchored at the model's `.prisma`
/// declaration line), these fire at a BE-source call site, so `file`/`line` point there directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinIssue {
    /// Bare rule id ("soft-delete-bypass" | "orderby-unindexed"), with no `"schema/"` pack-namespace prefix.
    pub rule: String,
    pub severity: Severity,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub file: String,
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

fn has_attr(model: &SchemaModel, field_name: &str, attr: &str) -> bool {
    model
        .fields
        .iter()
        .find(|f| f.name == field_name)
        .is_some_and(|f| f.attrs.iter().any(|a| a.name == attr))
}

fn leading_column(groups: &[Vec<String>], field_name: &str) -> bool {
    groups
        .iter()
        .any(|g| g.first().map(String::as_str) == Some(field_name))
}

fn find_model<'a>(models: &'a [SchemaModel], name: &str) -> Option<&'a SchemaModel> {
    models.iter().find(|m| m.name == name)
}

/// The soft-delete marker field name on a model, if any (`deletedAt` or `deleted_at`).
fn soft_delete_field(model: &SchemaModel) -> Option<&str> {
    model
        .fields
        .iter()
        .find(|f| f.name == "deletedAt" || f.name == "deleted_at")
        .map(|f| f.name.as_str())
}

/// `soft-delete-bypass`: for each model with a soft-delete marker field, flags every
/// `findMany`/`findFirst`/`findUnique`/`count` call site whose argument span never mentions the field name.
/// Purely lexical, so a Prisma middleware or `$extends` client extension injecting a global filter is
/// invisible to it (stated in the finding message too) — a repo relying on one should disable this rule id.
pub fn soft_delete_bypass_issues(
    models: &[SchemaModel],
    sites: &[QueryCallSite],
) -> Vec<JoinIssue> {
    let mut out = Vec::new();
    for model in models {
        let Some(field_name) = soft_delete_field(model) else {
            continue;
        };
        let word_re = Regex::new(&format!(r"\b{}\b", regex::escape(field_name))).unwrap();
        for site in sites {
            // `sites` arrives pre-filtered to the 4 query methods by
            // `zzop_parser_typescript::extract_query_call_sites` (that crate's `QUERY_METHODS` is the
            // single source of truth) — only the model needs checking here.
            if site.model != model.name {
                continue;
            }
            if word_re.is_match(&site.call_text) {
                continue;
            }
            out.push(JoinIssue {
                rule: "soft-delete-bypass".to_string(),
                severity: Severity::Warning,
                model: model.name.clone(),
                field: Some(field_name.to_string()),
                file: site.file.clone(),
                line: site.line,
                params: Some(serde_json::json!({ "method": site.method })),
            });
        }
    }
    out.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
    out
}

/// Matches a single-field `orderBy: { field: 'asc' | "desc" }` object literal — a trailing comma before the
/// closing `}` is tolerated, but a second key is not. Multi-key objects and the `orderBy: [...]` array form
/// both fail to match and are silently skipped, not misread as single-field.
fn single_field_order_by(call_text: &str) -> Option<String> {
    let re = order_by_re();
    re.captures(call_text).map(|c| c[1].to_string())
}

fn order_by_re() -> &'static Regex {
    static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r#"orderBy\s*:\s*\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*:\s*['"]?(?:asc|desc)['"]?\s*,?\s*\}"#,
        )
        .unwrap()
    })
}

/// True if `field_name` already has index coverage on `model`: its own `@id`/`@unique`, or the leading
/// column of any `@@index`/`@@unique` block (a leading column is usable for a single-column sort the same
/// way a dedicated index would be; a trailing column is not, per standard B-tree semantics).
fn field_index_covered(model: &SchemaModel, field_name: &str) -> bool {
    has_attr(model, field_name, "id")
        || has_attr(model, field_name, "unique")
        || leading_column(&model.indexes, field_name)
        || leading_column(&model.uniques, field_name)
}

/// `orderby-unindexed`: a single-field literal `orderBy` naming a field with no `@id`/`@unique`/
/// leading-`@@index` coverage on a resolvable model. Multi-field/array `orderBy`, or a field name that
/// doesn't resolve to a declared field on the target model, are silently skipped rather than guessed at.
pub fn orderby_unindexed_issues(models: &[SchemaModel], sites: &[QueryCallSite]) -> Vec<JoinIssue> {
    let mut out = Vec::new();
    for site in sites {
        let Some(model) = find_model(models, &site.model) else {
            continue;
        };
        let Some(field_name) = single_field_order_by(&site.call_text) else {
            continue;
        };
        if !model.fields.iter().any(|f| f.name == field_name) {
            continue; // not a declared field on this model -> unresolvable, skip (decidable-subset boundary).
        }
        if field_index_covered(model, &field_name) {
            continue;
        }
        out.push(JoinIssue {
            rule: "orderby-unindexed".to_string(),
            severity: Severity::Warning,
            model: model.name.clone(),
            field: Some(field_name),
            file: site.file.clone(),
            line: site.line,
            params: Some(serde_json::json!({ "method": site.method })),
        });
    }
    out.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
    out
}

/// Field name -> the SINGLE enum type it resolves to across every model in `models`, or `None` when that
/// field name is enum-typed to two-or-more DIFFERENT enums (ambiguous — `enum_string_drift_issues` skips
/// it entirely rather than guessing which enum's members apply).
fn resolve_unambiguous_enum_fields(
    models: &[SchemaModel],
    enums: &[SchemaEnum],
) -> HashMap<String, Option<SchemaEnum>> {
    let mut map: HashMap<String, Option<SchemaEnum>> = HashMap::new();
    for model in models {
        for field in &model.fields {
            let Some(en) = enums.iter().find(|e| e.name == field.r#type) else {
                continue;
            };
            map.entry(field.name.clone())
                .and_modify(|existing| {
                    if let Some(cur) = existing.as_ref() {
                        if cur.name != en.name {
                            *existing = None; // ambiguous: two different enum types under one field name.
                        }
                    }
                })
                .or_insert_with(|| Some(en.clone()));
        }
    }
    map
}

/// Every literal directly assigned to `field_name: 'Literal'` in `call_text` — a literal inside
/// `in: [...]`, a bare identifier, or a computed expression is silently skipped by design.
fn literal_matches(call_text: &str, field_name: &str) -> Vec<String> {
    let re = Regex::new(&format!(
        r#"\b{}\s*:\s*['"]([^'"]*)['"]"#,
        regex::escape(field_name)
    ))
    .unwrap();
    re.captures_iter(call_text)
        .map(|c| c[1].to_string())
        .collect()
}

/// `enum-string-drift`: see this module's doc for the full decidable-subset boundary. Empty immediately
/// when `enums` is empty (schema declares no enum at all — nothing to join against).
pub fn enum_string_drift_issues(
    models: &[SchemaModel],
    enums: &[SchemaEnum],
    sites: &[QueryCallSite],
) -> Vec<JoinIssue> {
    if enums.is_empty() {
        return Vec::new();
    }
    let field_enum = resolve_unambiguous_enum_fields(models, enums);
    let mut out = Vec::new();
    for site in sites {
        let Some(model) = find_model(models, &site.model) else {
            continue;
        };
        for field in &model.fields {
            let Some(maybe_en) = field_enum.get(&field.name) else {
                continue;
            };
            let Some(en) = maybe_en else {
                continue; // ambiguous field name across models -> skip (conservative).
            };
            if field.r#type != en.name {
                continue; // this model's field with that name isn't itself enum-typed (name collision).
            }
            let mut literals: Vec<String> = literal_matches(&site.call_text, &field.name)
                .into_iter()
                .filter(|lit| !en.members.iter().any(|m| m == lit))
                .collect();
            literals.sort();
            literals.dedup();
            for lit in literals {
                out.push(JoinIssue {
                    rule: "enum-string-drift".to_string(),
                    severity: Severity::Warning,
                    model: model.name.clone(),
                    field: Some(field.name.clone()),
                    file: site.file.clone(),
                    line: site.line,
                    params: Some(
                        serde_json::json!({ "enum": en.name, "literal": lit, "method": site.method }),
                    ),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        let ka = (a.file.as_str(), a.line, a.field.as_deref().unwrap_or(""));
        let kb = (b.file.as_str(), b.line, b.field.as_deref().unwrap_or(""));
        ka.cmp(&kb)
    });
    out
}

#[cfg(test)]
mod tests;
