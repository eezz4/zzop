//! The schema x usage JOIN native rules — see `run_schema_join_rules`'s doc.

use std::collections::HashMap;
use std::time::Instant;

use zzop_core::{is_enabled, Finding};

use crate::analyze::record_native_timing;
use crate::EngineConfig;

/// Runs the three schema x usage JOIN native rules (`soft-delete-bypass` / `orderby-unindexed` /
/// `enum-string-drift` — `zzop_rules_schema::join`'s module doc) — a whole-tree pass over every
/// non-degraded Prisma file (`prisma_rels`, same eligibility as `schema-usage`) plus `sites`, every
/// file's Prisma query-call-site facts already collected by `assemble`'s per-artifact loop (parser
/// output, not a filesystem walk of this function's own — see `zzop_parser_typescript::
/// extract_query_call_sites`), gated per-id via `is_enabled` and timed via `record_native_timing`, the
/// same shape every other whole-tree native analysis in `assemble` uses.
///
/// `enum-string-drift` also collects `SchemaEnum`s (via `zzop_parser_prisma::parse_schema_enums`,
/// alongside the per-file `parse_schema` call for models) over the same `prisma_rels`, so
/// `enum_string_drift_issues` has both model and enum substrate to join call-site literals against.
///
/// All three rules need evidence spanning the whole BE source tree (every query call site for a model,
/// not just one file), so the model/enum parse is recomputed in full on every `assemble` call and never
/// enters the per-file findings cache (`sites` itself IS cached, per-file, via `FileIrSlice`).
pub(in crate::analyze) fn run_schema_join_rules(
    root: &std::path::Path,
    prisma_rels: &[String],
    sites: &[zzop_core::QueryCallSite],
    config: &EngineConfig,
    profile: bool,
    rule_time: &mut HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
) {
    if prisma_rels.is_empty() {
        return;
    }
    if !is_enabled(&config.rule_config, "soft-delete-bypass")
        && !is_enabled(&config.rule_config, "orderby-unindexed")
        && !is_enabled(&config.rule_config, "enum-string-drift")
    {
        return;
    }

    let mut models: Vec<zzop_core::SchemaModel> = Vec::new();
    let mut enums: Vec<zzop_core::SchemaEnum> = Vec::new();
    for rel in prisma_rels {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        models.extend(zzop_parser_prisma::parse_schema(&text, Some(rel), None));
        enums.extend(zzop_parser_prisma::parse_schema_enums(&text));
    }
    if models.is_empty() {
        return;
    }

    run_join_rule(
        "soft-delete-bypass",
        &config.rule_config,
        profile,
        &models,
        sites,
        zzop_rules_schema::soft_delete_bypass_issues,
        rule_time,
        global_findings,
    );
    run_join_rule(
        "orderby-unindexed",
        &config.rule_config,
        profile,
        &models,
        sites,
        zzop_rules_schema::orderby_unindexed_issues,
        rule_time,
        global_findings,
    );
    run_join_rule(
        "enum-string-drift",
        &config.rule_config,
        profile,
        &models,
        sites,
        |m, s| zzop_rules_schema::enum_string_drift_issues(m, &enums, s),
        rule_time,
        global_findings,
    );
}

/// Runs one schema x usage JOIN rule (`rule_fn`) under the `id` gate, appending its findings to
/// `global_findings` and timing the call. `rule_fn` is generic (not a bare `fn` pointer) so
/// `enum-string-drift`'s call site can close over its extra `enums` argument via a closure while the
/// other two rules' plain `fn` items keep coercing in unchanged.
#[allow(clippy::too_many_arguments)]
fn run_join_rule<F>(
    id: &str,
    rule_config: &zzop_core::RuleConfig,
    profile: bool,
    models: &[zzop_core::SchemaModel],
    sites: &[zzop_rules_schema::QueryCallSite],
    rule_fn: F,
    rule_time: &mut HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
) where
    F: Fn(
        &[zzop_core::SchemaModel],
        &[zzop_rules_schema::QueryCallSite],
    ) -> Vec<zzop_rules_schema::JoinIssue>,
{
    if !is_enabled(rule_config, id) {
        return;
    }
    let t0 = profile.then(Instant::now);
    let issues = rule_fn(models, sites);
    let found: Vec<Finding> = issues.iter().map(join_issue_to_finding).collect();
    record_native_timing(rule_time, t0, id, found.len());
    global_findings.extend(found);
}

/// One `JoinIssue` -> one `Finding`. Unlike `schema_issue_to_finding` (`pipeline.rs`), no
/// `zzop_parser_prisma::model_decl_line` lookup is needed: `JoinIssue` already carries the exact BE
/// call-site `file`/`line` it fired at. `rule_id` is the bare id, not `"schema/{id}"` — each of these
/// three is a whole individually-gated toggle unit, matching `duplicate-route`'s convention rather than
/// `schema-usage`'s pack-namespace-prefixed sub-rule ids.
fn join_issue_to_finding(issue: &zzop_rules_schema::JoinIssue) -> Finding {
    Finding {
        rule_id: issue.rule.clone(),
        severity: issue.severity,
        file: issue.file.clone(),
        line: issue.line,
        message: zzop_rules_schema::join_issue_message(issue),
        data: serde_json::to_value(issue).ok(),
    }
}
