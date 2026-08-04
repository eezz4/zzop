//! zzop-rules-schema — native Prisma schema rules: structural anti-patterns plus usage-aware cross-checks.
//! Schema IR types (`SchemaModel`, `SchemaField`, `FieldAttr`, `SchemaUsage`) live in `zzop-core`, shared
//! between `zzop-parser-prisma` (which builds them) and `zzop-engine`/this crate (which consume them);
//! everything that operates on that IR — rule bodies, usage-evidence collectors, message vocabulary —
//! lives here instead.
//!
//! - [`structural`]: the 9 structural rules, keyed off a model's own declaration (god-model,
//!   missing-timestamps, redundant-index, float-money, stale-updated-at, temporal-as-string, fk-no-index,
//!   nullable-fk, implicit-fk).
//! - [`usage`]: usage-evidence collectors (per-file field-usage tokens) and the rules built on them
//!   (unreferenced-model-name, unreferenced-field-name, model-churn). The first two were spelled
//!   `dead-model`/`dead-field` until they became registered ids; the matcher only ever asked whether the
//!   NAME occurs as an identifier token in this tree's `.ts`/`.tsx` files, which is not the same claim as
//!   "dead" — the ids now say what is measured (old ids recorded in `VERSIONING.md`).
//!   `unreferenced-model-name` keys on the generic vocab-free signal (is the
//!   model name referenced in source?); store-binding/migration-churn are read off the generic
//!   entity-attribute channel (`zzop_core::AttributeStore`, Symbol-keyed `bound-model`/`model-churn`) now
//!   (the app-specific store-binding recognizer and migration-history FS-walk were removed as native
//!   over-reach).
//! - [`join`]: schema x usage JOIN rules (soft-delete-bypass, orderby-unindexed, enum-string-drift),
//!   anchored at the query call site instead of the model declaration.
//! - [`message`]: the human-facing prose for every rule id above.
//!
//! Every issue label the [`structural`]/[`usage`] modules emit is a REGISTERED rule id of its own, spelled
//! `schema/<label>` (see [`register_native_analyses`] and [`schema_issue_rule_id`]) — not the family gate
//! alone. The two family gates remain, and remain the way to switch a whole pass off.
//!
//! This crate depends only on `zzop-core`, never on `zzop-parser-prisma`. The reverse edge used to exist
//! in production — `zzop-parser-prisma` called `analyze_schema` for a bundled schema-analysis capability
//! of its own — but that entry point had zero callers and was deleted 2026-07-27, so the edge is now a
//! `dev-dependency` carrying one vertical-slice test. Wiring parse -> rules is `zzop-engine`'s job, which
//! is also where the raw-source line-number lookup some findings need stays, to avoid a dependency cycle.

pub mod join;
pub mod message;
pub mod structural;
pub mod usage;

use zzop_core::{register_native_analysis_stub, RuleRegistry};

/// The namespace a `SchemaIssue`'s label wears inside a finding's `ruleId`. The engine's
/// `schema_issue_to_finding` composes exactly `<namespace><label>`, and [`schema_issue_rule_id`] is the one
/// function that composition and this crate's registration both go through — so the registered id and the
/// id a user reads out of real output cannot drift apart.
pub const SCHEMA_ISSUE_NAMESPACE: &str = "schema/";

/// The labels [`structural::apply_schema_rules`] emits, in declaration order. Registered as
/// `schema/<label>` (see [`register_native_analyses`]) AND consulted by [`message::schema_issue_message`]
/// to pick the family gate — one list, two readers, so a label cannot be registered without a message or
/// carry a message without being registered.
pub const SCHEMA_STRUCTURAL_ISSUE_LABELS: [&str; 9] = [
    "god-model",
    "missing-timestamps",
    "redundant-index",
    "float-money",
    "stale-updated-at",
    "temporal-as-string",
    "fk-no-index",
    "nullable-fk",
    "implicit-fk",
];

/// The labels [`usage::cross_check_schema`]/[`usage::apply_churn_rule`] emit. Same dual readership as
/// [`SCHEMA_STRUCTURAL_ISSUE_LABELS`].
pub const SCHEMA_USAGE_ISSUE_LABELS: [&str; 3] = [
    "unreferenced-model-name",
    "unreferenced-field-name",
    "model-churn",
];

/// The registered rule id a `SchemaIssue` label reports under — `schema/god-model`, ... The engine builds a
/// finding's `rule_id` with this, `register_native_analyses` registers exactly these strings, and
/// `registry::is_enabled` is consulted on the result at both schema call sites, so the id a user copies out
/// of `ruleId` is the id `disabledRules`/`severityOverrides`/`suppressions` take.
pub fn schema_issue_rule_id(label: &str) -> String {
    format!("{SCHEMA_ISSUE_NAMESPACE}{label}")
}

/// Registers every native analysis id implemented in this crate — the schema half of the extensibility
/// contract's per-crate registration (see `zzop_engine::register_all_native`, which composes this with
/// `zzop_rules_graph`'s, `zzop_rules_http`'s, `zzop_rules_cross_layer`'s, and `zzop_metrics`'s own
/// `register_native_analyses`).
///
/// Three groups: the two FAMILY gates (`schema-structural`/`schema-usage`, which switch a whole pass on or
/// off and stay the cheap way to silence all of it), the three JOIN rules, and the 12 per-issue ids the two
/// families report under. The 12 used to be labels with no registered id at all — `ruleId` said
/// `schema/god-model` while the config id space knew only `schema-structural`, so a user copying the id out
/// of output got "unknown rule id" from `zzop explain`, an "unknown disabled rule" warning from a config
/// that then did nothing, and — worse — an "unknown severity override" warning about an override
/// `apply_severity_override` was in fact honoring (it matches `Finding::rule_id` exactly).
pub fn register_native_analyses(registry: &mut RuleRegistry) {
    for id in [
        "schema-structural",
        "schema-usage",
        "soft-delete-bypass",
        "orderby-unindexed",
        "enum-string-drift",
    ] {
        register_native_analysis_stub(registry, id);
    }
    for label in SCHEMA_STRUCTURAL_ISSUE_LABELS
        .iter()
        .chain(SCHEMA_USAGE_ISSUE_LABELS.iter())
    {
        register_native_analysis_stub(registry, &schema_issue_rule_id(label));
    }
}

pub use join::{
    enum_string_drift_issues, orderby_unindexed_issues, soft_delete_bypass_issues, JoinIssue,
    QueryCallSite,
};
pub use message::{join_issue_message, rule_sightlines, schema_issue_message};
pub use structural::{
    analyze_schema, apply_schema_rules, SchemaAnalysis, SchemaIssue, MONEY_TOKENS,
    STRUCTURAL_RULES_VERSION,
};
pub use usage::{
    analyze_schema_with_usage, apply_churn_rule, cross_check_schema, field_usage_tokens,
    SKIP_FIELD_NAMES,
};
