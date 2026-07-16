//! Prisma schema-usage analysis — usage-evidence collectors (per-file field-usage tokens) plus the usage-aware cross-check layered on top of the structural analyzer in `structural.rs`.
//! `SchemaUsage` (the usage-evidence IR a producer assembles) lives in `zzop-core`; every function that consumes or produces it lives here. `analyze_schema_with_usage` wraps `structural::analyze_schema`
//! rather than modifying it, layering cross-check/churn issues and risk points on top. `structural::severity_points` is private to `structural.rs`, so it's duplicated here — keep the two in sync.
//!
//! `identifier_counts` evidence comes from a per-file fact carried through `zzop_engine`'s fused per-file pass: [`field_usage_tokens`] (this module) is the direct per-file substrate, called once per file
//! with the text that pass already has in hand (no filesystem re-walk). Store-binding and migration-churn are environment facts about a specific project's architecture (a store-binding convention, a
//! migration-history layout); per the "native = common environments only, everything else injected" line, their app-specific native recognizers were removed — both are now read off the generic
//! entity-attribute channel (`zzop_core::AttributeStore`, Symbol-keyed [`BOUND_MODEL_ATTR`]/[`MODEL_CHURN_ATTR`]) rather than typed `SchemaUsage` slots. `dead-model` therefore keys on the generic
//! vocab-free signal (is the model name referenced anywhere?) plus whatever a producer injects into `BOUND_MODEL_ATTR`.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

use zzop_core::{AttributeStore, SchemaModel, SchemaUsage, Severity};

use crate::structural::{analyze_schema, SchemaAnalysis, SchemaIssue};

/// Attribute key a producer/overlay sets on a model `Symbol` to assert a store/repository binding exists
/// (suppresses dead-model). The retrofit of the removed native store-binding recognizer onto the generic
/// entity-attribute channel — dead-model now reads this instead of `SchemaUsage.bound_models`.
pub const BOUND_MODEL_ATTR: &str = "bound-model";
/// Attribute key a producer/overlay sets on a model `Symbol` carrying that model's cumulative migration
/// churn count (a number). Drives schema-churn. Replaces the removed `SchemaUsage.model_churn` slot.
pub const MODEL_CHURN_ATTR: &str = "model-churn";

macro_rules! lazy_re {
    ($f:ident, $p:expr) => {
        fn $f() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| Regex::new($p).unwrap())
        }
    };
}

// --- fieldUsageTokens (replaces the removed scanFieldUsage filesystem walk) ---

/// Comment/string-stripped identifier tokens referenced anywhere in one file's raw text — the direct
/// per-file substrate `zzop_engine`'s fused per-file pass now feeds into `SchemaUsage.identifier_counts`
/// (each file's set unioned tree-wide, then re-counted to presence — see that crate's `assemble`).
/// Replaces the removed `scan_field_usage`'s own `<root>/src` filesystem walk: same recognizer (plain
/// identifier tokens on comment/string-stripped text — common names like id/name appear everywhere, so
/// they're effectively never flagged dead, keeping false positives low at the cost of recall), just
/// invoked once per file instead of via a second full-tree walk. `rel` gates which files are worth
/// scanning at all (see [`is_field_usage_scan_file`]); an excluded file yields an empty set regardless of
/// `text`.
pub fn field_usage_tokens(rel: &str, text: &str) -> HashSet<String> {
    if !is_field_usage_scan_file(rel) {
        return HashSet::new();
    }
    let stripped = strip_comments_and_strings(text);
    ident_re()
        .find_iter(&stripped)
        .map(|m| m.as_str().to_string())
        .collect()
}

/// `.ts`/`.tsx` only, excluding `.d.ts` declaration files — mirrors the removed `walk_ts_files`'s own
/// per-file filename filter. The old walk also hard-excluded `node_modules`/`dist`/`data` directories;
/// that exclusion isn't reproduced here since the fused per-file pass this now runs inside already skips
/// `node_modules`/`dist` under the DEFAULT `skip_dirs` (`EngineConfig`) — a subset of the old exclusions,
/// so under default config the fused pass covers every file the old `<root>/src` walk did plus more,
/// which only ADDS identifier evidence (the accepted tree-wide-widening deviation, see module doc) and
/// never adds a false dead-field positive. Caveat: a MORE-aggressive custom `skip_dirs` could exclude a
/// source dir the old walk scanned, dropping "used" tokens and potentially surfacing a false dead-field —
/// acceptable, since a user who scopes analysis away from a directory is opting out of its evidence.
fn is_field_usage_scan_file(rel: &str) -> bool {
    if rel.ends_with(".d.ts") {
        return false;
    }
    rel.ends_with(".ts") || rel.ends_with(".tsx")
}

fn strip_comments_and_strings(src: &str) -> String {
    let no_block = block_comment_re().replace_all(src, " ");
    let no_line = line_comment_re().replace_all(&no_block, "$1");
    let no_dq = double_quote_re().replace_all(&no_line, "\"\"");
    let no_sq = single_quote_re().replace_all(&no_dq, "''");
    template_re().replace_all(&no_sq, "``").into_owned()
}

lazy_re!(block_comment_re, r"(?s)/\*.*?\*/");
lazy_re!(line_comment_re, r"(?m)(^|[^:])//.*$");
lazy_re!(double_quote_re, r#""(?:\\.|[^"\\])*""#);
lazy_re!(single_quote_re, r"'(?:\\.|[^'\\])*'");
lazy_re!(template_re, r"`(?:\\.|[^`\\])*`");
// ASCII-only identifier token, mirroring JS `\b[a-zA-Z_$][\w$]*\b` (JS `\w` is ASCII-only).
lazy_re!(ident_re, r"[A-Za-z_$][A-Za-z0-9_$]*");

// Migration churn (`MODEL_CHURN_ATTR`) is an environment fact — accumulated schema-change history that
// lives in migration files the parse pass never dispatches, under a deployment-specific directory layout.
// Per the "native = common environments only; everything else is injected" design line, a native
// recognizer for it (the removed `scan_migration_churn`, which FS-walked a
// `<root>/src/domains/*/prisma/migrations/` layout and regex-re-parsed raw `.sql` off disk — both a
// rule-side re-parse leak AND a one-project layout) has no place here. `MODEL_CHURN_ATTR` is the injection
// slot instead — a producer that knows a project's migration layout injects it on the model `Symbol`, and
// `apply_churn_rule` (below) reads it off the generic entity-attribute channel.

// --- crossCheckSchema + applyChurnRule + analyzeSchema (usage branch) ---

const SKIP_FIELD_NAMES: [&str; 3] = ["id", "createdAt", "updatedAt"];
/// Very short field names appear everywhere in BE source; dead-field detection is meaningless -> exclude.
const MIN_FIELD_NAME_LEN: usize = 3;

/// Schema cross-check — compares the schema-IR against actual BE code usage. Surfaces dead-model (a model not bound to any store) and dead-field (a field never appearing as an identifier in BE source)
/// issues. id/createdAt/updatedAt are excluded by default since infrastructure fields are rarely referenced directly.
pub fn cross_check_schema(
    models: &[SchemaModel],
    usage: &SchemaUsage,
    attrs: &AttributeStore,
) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    for model in models {
        // A model is "used" if its name appears as an identifier anywhere in BE source
        // (`identifier_counts`, the generic vocab-free signal — same substrate dead-field uses), OR if a
        // Mode-B producer injected a truthy `BOUND_MODEL_ATTR` on the model's `Symbol` through the generic
        // entity-attribute channel. That channel is empty under native analysis now that the app-specific
        // store-binding recognizer is gone. This makes dead-model a general "the model name is never
        // referenced" check instead of "the model isn't wired through one project's store convention."
        let referenced = usage
            .identifier_counts
            .get(&model.name)
            .copied()
            .unwrap_or(0)
            > 0;
        let bound = attrs
            .symbol_attr(&model.name, None, BOUND_MODEL_ATTR)
            .is_some_and(zzop_core::attr_is_truthy);
        if !referenced && !bound {
            issues.push(SchemaIssue {
                rule: "dead-model".to_string(),
                severity: Severity::Info,
                model: model.name.clone(),
                field: None,
                params: None,
            });
            continue;
        }
        for field in &model.fields {
            if SKIP_FIELD_NAMES.contains(&field.name.as_str()) {
                continue;
            }
            if field.name.len() < MIN_FIELD_NAME_LEN {
                continue;
            }
            if usage
                .identifier_counts
                .get(&field.name)
                .copied()
                .unwrap_or(0)
                > 0
            {
                continue;
            }
            issues.push(SchemaIssue {
                rule: "dead-field".to_string(),
                severity: Severity::Info,
                model: model.name.clone(),
                field: Some(field.name.clone()),
                params: None,
            });
        }
    }
    issues
}

const CHURN_WARNING_THRESHOLD: u32 = 5;
const CHURN_CRITICAL_THRESHOLD: u32 = 10;

/// schema-churn rule — detects design instability from accumulated migration churn on a model. Churn count
/// per model is read off the generic entity-attribute channel (`MODEL_CHURN_ATTR` on the model's `Symbol`);
/// a model with no injected churn attribute is treated as zero, so this self-gates and is safe to always call.
pub fn apply_churn_rule(models: &[SchemaModel], attrs: &AttributeStore) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    for model in models {
        let count = attrs
            .symbol_attr(&model.name, None, MODEL_CHURN_ATTR)
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        if count < CHURN_WARNING_THRESHOLD {
            continue;
        }
        let severity = if count >= CHURN_CRITICAL_THRESHOLD {
            Severity::Critical
        } else {
            Severity::Warning
        };
        issues.push(SchemaIssue {
            rule: "schema-churn".to_string(),
            severity,
            model: model.name.clone(),
            field: None,
            params: Some(serde_json::json!({ "count": count })),
        });
    }
    issues
}

/// Mirrors `structural::severity_points`, which is private to `structural.rs` (see module doc).
fn severity_points(s: Severity) -> i64 {
    match s {
        Severity::Critical => 5,
        Severity::Warning => 2,
        Severity::Info => 1,
    }
}

/// Usage-aware schema analysis: schema-IR (+ optional usage) -> `SchemaAnalysis` with a `model_risk` rollup. Always runs the structural rules; when `usage` is present, also runs `cross_check_schema` and
/// `apply_churn_rule` (self-gating: a model with no injected `MODEL_CHURN_ATTR` yields count 0 -> no issue), folding their risk points into `model_risk`.
pub fn analyze_schema_with_usage(
    models: Vec<SchemaModel>,
    usage: Option<SchemaUsage>,
    attrs: &AttributeStore,
) -> SchemaAnalysis {
    let mut analysis = analyze_schema(models);
    let Some(usage) = usage else {
        return analysis;
    };
    let mut extra = cross_check_schema(&analysis.models, &usage, attrs);
    extra.extend(apply_churn_rule(&analysis.models, attrs));
    for issue in &extra {
        *analysis.model_risk.entry(issue.model.clone()).or_insert(0) +=
            severity_points(issue.severity);
    }
    analysis.issues.extend(extra);
    analysis
}

#[cfg(test)]
mod tests;
