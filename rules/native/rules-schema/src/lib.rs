//! zzop-rules-schema — native Prisma schema rules: structural anti-patterns plus usage-aware cross-checks.
//! Schema IR types (`SchemaModel`, `SchemaField`, `FieldAttr`, `SchemaUsage`) live in `zzop-core`, shared
//! between `zzop-parser-prisma` (which builds them) and `zzop-engine`/this crate (which consume them);
//! everything that operates on that IR — rule bodies, usage-evidence collectors, message vocabulary —
//! lives here instead.
//!
//! - [`structural`]: the 9 structural rules, keyed off a model's own declaration (god-model,
//!   missing-timestamps, redundant-index, float-money, stale-updated-at, temporal-as-string, fk-no-index,
//!   nullable-fk, implicit-fk).
//! - [`usage`]: usage-evidence collectors (field usage, migration churn, store map) and the rules built on
//!   them (dead-model, dead-field, schema-churn).
//! - [`join`]: schema x usage JOIN rules (soft-delete-bypass, orderby-unindexed, enum-string-drift),
//!   anchored at the query call site instead of the model declaration.
//! - [`message`]: the human-facing prose for every rule id above.
//!
//! This crate depends only on `zzop-core`, never on `zzop-parser-prisma`, even though `zzop-parser-prisma`
//! depends on this crate to expose its own bundled schema-analysis capability — a raw-source line-number
//! lookup some findings need stays in `zzop-engine` instead, to avoid the resulting dependency cycle.

pub mod join;
pub mod message;
pub mod structural;
pub mod usage;

use zzop_core::{register_native_analysis_stub, RuleRegistry, Severity};

/// Registers every native analysis id implemented in this crate — the schema half of the extensibility
/// contract's per-crate registration (see `zzop_engine::register_all_native`, which composes this with
/// `zzop_rules_graph`'s and `zzop_metrics`'s own `register_native_analyses`).
pub fn register_native_analyses(registry: &mut RuleRegistry) {
    let analyses: &[(&str, Severity)] = &[
        ("schema-structural", Severity::Warning),
        ("schema-usage", Severity::Warning),
        ("soft-delete-bypass", Severity::Warning),
        ("orderby-unindexed", Severity::Warning),
        ("enum-string-drift", Severity::Warning),
    ];
    for &(id, default_severity) in analyses {
        register_native_analysis_stub(registry, id, default_severity);
    }
}

pub use join::{
    enum_string_drift_issues, orderby_unindexed_issues, scan_query_call_sites,
    soft_delete_bypass_issues, JoinIssue, QueryCallSite,
};
pub use message::{join_issue_message, schema_issue_message};
pub use structural::{
    analyze_schema, apply_schema_rules, SchemaAnalysis, SchemaIssue, STRUCTURAL_RULES_VERSION,
};
pub use usage::{
    analyze_schema_with_usage, apply_churn_rule, cross_check_schema, scan_field_usage,
    scan_migration_churn, scan_store_map,
};
