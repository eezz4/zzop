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
mod tests {
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
}
