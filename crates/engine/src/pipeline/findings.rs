//! Per-file DSL pack evaluation and native schema-rule wiring (structural + usage cross-check).

use std::fs;
use std::path::Path;

use zzop_core::{
    dsl::{eval_pack, eval_pack_profiled, RuleContext, RuleTiming, SourceFile},
    ir::SourceSymbol,
    pack_loader, registry, IoFacts, RulePackDef,
};

use crate::dispatch::Language;

/// Runs every applicable DSL pack against this one file's slice. `packs` is already
/// `is_enabled`-filtered by `run_file_pass`; `pack_loader::applies_to` is the remaining per-file,
/// per-pack pre-filter. Short-circuits before iterating `packs` when the text is minified/generated
/// (skips all matcher types, not only line-scan); the returned bool lets callers set
/// `FileArtifact::minified_or_generated` without recomputing the check.
///
/// `profile` mirrors `EngineConfig::profile_rules`: `false` calls `eval_pack` (no timing overhead);
/// `true` calls `eval_pack_profiled` and concatenates every pack's `RuleTiming`s, summed later across
/// every artifact by `analyze::assemble`.
///
/// D13①: every DSL finding this returns has `zzop_core::disable_hint`'s config-disable fragment appended
/// to its `message`, AFTER the pack's own suppress-marker sentence — the same hint native findings carry
/// (see `disable_hint`'s doc), built from the SAME helper (never a second hand-written template). This is
/// the single per-file DSL finding-construction site in the engine's fused pass — `envelope::file_pass`
/// (Mode B's own `eval_pack` call site) appends identically, via the same helper, since it never routes
/// through this function. `Finding::rule_id` is already `"<pack>/<rule>"` (stamped inside `eval_pack`
/// itself), so no extra id plumbing is needed here.
///
/// This runs BEFORE the caller (`fresh::compute_fresh_artifact` / `artifact::process_file`) hands
/// `findings` to `AnalysisCache::put_findings` — the hint text is therefore part of the cached findings
/// entry's `message` field, not appended fresh on every cache hit. See `cache.rs`'s `CACHE_SCHEMA_VERSION`
/// doc for the schema bump this required.
pub(super) fn eval_packs(
    packs: &[&RulePackDef],
    rel: &str,
    text: &str,
    symbols: &[SourceSymbol],
    io: Option<IoFacts>,
    loop_spans: &[(u32, u32)],
    profile: bool,
) -> (Vec<zzop_core::Finding>, Vec<RuleTiming>, bool) {
    if zzop_core::dsl::is_minified_or_generated(text) {
        return (Vec::new(), Vec::new(), true);
    }
    let file = SourceFile {
        loop_spans: loop_spans.to_vec(),
        rel: rel.to_string(),
        text: text.to_string(),
        symbols: symbols.to_vec(),
        io,
    };
    let files = std::slice::from_ref(&file);
    let ctx = RuleContext { files, ir: None };
    let mut out = Vec::new();
    let mut timings = Vec::new();
    for pack in packs {
        if pack_loader::applies_to(pack, rel) {
            if profile {
                let (findings, t) = eval_pack_profiled(pack, &ctx);
                out.extend(findings);
                timings.extend(t);
            } else {
                out.extend(eval_pack(pack, &ctx));
            }
        }
    }
    append_disable_hints(&mut out);
    (out, timings, false)
}

/// Appends `zzop_core::disable_hint(&finding.rule_id)` to every finding's `message` — the one place both
/// DSL finding-construction call sites (this module's `eval_packs`, and `envelope::file_pass`'s direct
/// `eval_pack` call) route through the SAME hint text, never a hand-rolled second copy. Every element of
/// `findings` here is a freshly-built DSL finding (this function is only ever called on an `eval_pack`/
/// `eval_pack_profiled` result), so `rule_id` is always the `"<pack>/<rule>"` shape the hint expects.
pub(crate) fn append_disable_hints(findings: &mut [zzop_core::Finding]) {
    for finding in findings.iter_mut() {
        finding.message = format!(
            "{} {}",
            finding.message,
            zzop_core::disable_hint(&finding.rule_id)
        );
    }
}

/// Whether a Prisma file's schema-structural rules (`schema_findings`) should run — shared by
/// `compute_fresh_artifact` and `process_file`'s cache-reuse branch so re-enabling `schema-structural`
/// on a warm run doesn't silently drop findings for already-cached files. Only Prisma, non-degraded.
pub(super) fn schema_findings_eligible(language: Option<Language>, degraded: bool) -> bool {
    matches!(language, Some(Language::Prisma)) && !degraded
}

/// Wires `zzop_rules_schema::apply_schema_rules` into the fused per-file pass for Prisma files:
/// re-parses this file's `SchemaModel`s (cheap — same scan `parse_prisma` already ran) and converts
/// each `SchemaIssue` into a `zzop_core::Finding`, gated behind native id `"schema-structural"`.
/// `rule_id` is `"schema/{issue.rule}"`, a fresh namespace since this is native logic, not a DSL pack.
pub(super) fn schema_findings(
    rule_config: &zzop_core::RuleConfig,
    rel: &str,
    text: &str,
) -> Vec<zzop_core::Finding> {
    if !registry::is_enabled(rule_config, "schema-structural") {
        return Vec::new();
    }
    let models = zzop_parser_prisma::parse_schema(text, Some(rel), None);
    zzop_rules_schema::apply_schema_rules(&models)
        .iter()
        .map(|issue| schema_issue_to_finding(rel, text, issue))
        .collect()
}

/// The usage counterpart of `schema_findings`: wires the usage cross-check (dead-model / dead-field /
/// schema-churn) via `zzop_rules_schema::cross_check_schema`/`apply_churn_rule`. Unlike `schema_findings`
/// this is a whole-tree pass — usage evidence (identifier presence) spans every source file, so it runs
/// from `analyze::assemble`'s global stage and is recomputed each run, never entering the per-file
/// findings cache. `analyze_schema_with_usage` is deliberately not used here since it re-runs the
/// structural rules the per-file pass already emitted.
///
/// `used_names` is the tree-wide union `analyze::assemble` collects from every `FileArtifact`'s
/// `field_usage_tokens` (populated in the fused per-file pass) — no filesystem re-walk. `attrs` is the
/// generic entity-attribute channel (`zzop_core::AttributeStore`) — store-binding and migration-churn are
/// no longer typed `SchemaUsage` slots, they're Symbol-keyed attributes (`bound-model`/`model-churn`) a
/// Mode-B producer injects; empty under native analysis, so `cross_check_schema`'s dead-model keys on the
/// generic `identifier_counts` presence signal alone, and `apply_churn_rule` fires only when a producer
/// injects churn (previously it could never fire). Degraded `.prisma` files are excluded by the caller;
/// unreadable schema files are skipped.
pub(crate) fn schema_usage_findings(
    root: &Path,
    prisma_rels: &[String],
    attrs: &zzop_core::AttributeStore,
    used_names: &std::collections::HashSet<String>,
) -> Vec<zzop_core::Finding> {
    if prisma_rels.is_empty() {
        return Vec::new();
    }
    let mut texts: Vec<(String, String)> = Vec::new();
    let mut models: Vec<zzop_core::SchemaModel> = Vec::new();
    for rel in prisma_rels {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        models.extend(zzop_parser_prisma::parse_schema(&text, Some(rel), None));
        texts.push((rel.clone(), text));
    }
    if models.is_empty() {
        return Vec::new();
    }
    let usage = zzop_core::SchemaUsage {
        identifier_counts: used_names.iter().map(|name| (name.clone(), 1u32)).collect(),
    };
    let mut issues = zzop_rules_schema::cross_check_schema(&models, &usage, attrs);
    issues.extend(zzop_rules_schema::apply_churn_rule(&models, attrs));
    issues
        .iter()
        .map(|issue| {
            // A usage issue names its model; `source_path` (stamped by `parse_schema` above) picks the
            // file whose text anchors the finding line. Known limit: if two .prisma files declare the
            // same model name, both issues anchor on the first declaration.
            let rel = models
                .iter()
                .find(|m| m.name == issue.model)
                .and_then(|m| m.source_path.as_deref())
                .unwrap_or_else(|| texts[0].0.as_str());
            let text = texts
                .iter()
                .find(|(r, _)| r == rel)
                .map(|(_, t)| t.as_str())
                .unwrap_or_default();
            schema_issue_to_finding(rel, text, issue)
        })
        .collect()
}

/// One `SchemaIssue` -> one `Finding`. `line` uses `zzop_parser_prisma::model_decl_line` since
/// `SchemaIssue` carries no line number of its own (only `model`/`field` names). `data` embeds the
/// full `SchemaIssue` so a structured consumer can recover `field`/`params` without re-parsing
/// `message`.
///
/// This glue stays in this engine rather than `zzop-rules-schema`: it needs
/// `zzop_parser_prisma::model_decl_line`, and `zzop-rules-schema` deliberately does not depend on
/// `zzop-parser-prisma` (the dependency runs the other way) — this engine depends on both.
fn schema_issue_to_finding(
    rel: &str,
    text: &str,
    issue: &zzop_rules_schema::SchemaIssue,
) -> zzop_core::Finding {
    zzop_core::Finding {
        rule_id: format!("schema/{}", issue.rule),
        severity: issue.severity,
        file: rel.to_string(),
        line: zzop_parser_prisma::model_decl_line(text, &issue.model),
        message: zzop_rules_schema::schema_issue_message(issue),
        data: serde_json::to_value(issue).ok(),
    }
}

// The schema x usage JOIN native rules (`soft-delete-bypass` / `orderby-unindexed`) are wired in
// `analyze::run_schema_join_rules`, beside `schema-usage`/`duplicate-route` — the canonical whole-tree
// native-rule call site.
