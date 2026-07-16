//! Prisma schema structural rules — source-agnostic checks over the schema IR (`zzop_core::schema`).
//! IR types (`SchemaModel` etc.) live in `zzop-core`; the rule bodies that operate on them live here.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use zzop_core::{SchemaModel, Severity};

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

mod rules;
#[cfg(test)]
mod tests;

use rules::{
    is_fk_candidate, rule_fk_no_index, rule_float_money, rule_god_model, rule_implicit_fk,
    rule_missing_timestamps, rule_nullable_fk, rule_redundant_index, rule_stale_updated_at,
    rule_temporal_as_string,
};
