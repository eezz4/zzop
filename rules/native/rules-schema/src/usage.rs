//! Prisma schema-usage analysis — usage-evidence collectors (per-file field-usage tokens) plus the usage-aware cross-check layered on top of the structural analyzer in `structural.rs`.
//! `SchemaUsage` (the usage-evidence IR a producer assembles) lives in `zzop-core`; every function that consumes or produces it lives here. `analyze_schema_with_usage` wraps `structural::analyze_schema`
//! rather than modifying it, layering cross-check/churn issues and risk points on top. Risk points come from `structural::severity_points` directly — it is `pub(crate)`, and the copy that used to sit here (justified by a comment claiming it was private) was deleted on 2026-09-07.
//!
//! `identifier_counts` evidence comes from a per-file fact carried through `zzop_engine`'s fused per-file pass: [`field_usage_tokens`] (this module) is the direct per-file substrate, called once per file
//! with the text that pass already has in hand (no filesystem re-walk). Store-binding and migration-churn are environment facts about a specific project's architecture (a store-binding convention, a
//! migration-history layout); per the "native = common environments only, everything else injected" line, their app-specific native recognizers were removed — both are now read off the generic
//! entity-attribute channel (`zzop_core::AttributeStore`, Symbol-keyed [`BOUND_MODEL_ATTR`]/[`MODEL_CHURN_ATTR`]) rather than typed `SchemaUsage` slots. `unreferenced-model-name` therefore keys on the generic
//! vocab-free signal (is the model name referenced anywhere?) plus whatever a producer injects into `BOUND_MODEL_ATTR`.

use zzop_core::{AttributeStore, SchemaModel, SchemaUsage, Severity};

use crate::structural::{analyze_schema, severity_points, SchemaAnalysis, SchemaIssue};

/// Attribute key a producer/overlay sets on a model `Symbol` to assert a store/repository binding exists
/// (suppresses unreferenced-model-name). The retrofit of the removed native store-binding recognizer onto the generic
/// entity-attribute channel — unreferenced-model-name now reads this instead of `SchemaUsage.bound_models`.
pub const BOUND_MODEL_ATTR: &str = "bound-model";
/// Attribute key a producer/overlay sets on a model `Symbol` carrying that model's cumulative migration
/// churn count (a number). Drives model-churn. Replaces the removed `SchemaUsage.model_churn` slot.
pub const MODEL_CHURN_ATTR: &str = "model-churn";

// --- fieldUsageTokens (replaces the removed scanFieldUsage filesystem walk) ---

mod tokens;

pub use tokens::{field_usage_tokens, FIELD_USAGE_SCAN_EXTENSIONS};

// ASCII-only identifier token, mirroring JS `\b[a-zA-Z_$][\w$]*\b` (JS `\w` is ASCII-only).
// Migration churn (`MODEL_CHURN_ATTR`) is an environment fact — accumulated schema-change history that
// lives in migration files the parse pass never dispatches, under a deployment-specific directory layout.
// Per the "native = common environments only; everything else is injected" design line, a native
// recognizer for it (the removed `scan_migration_churn`, which FS-walked a
// `<root>/src/domains/*/prisma/migrations/` layout and regex-re-parsed raw `.sql` off disk — both a
// rule-side re-parse leak AND a one-project layout) has no place here. `MODEL_CHURN_ATTR` is the injection
// slot instead — a producer that knows a project's migration layout injects it on the model `Symbol`, and
// `apply_churn_rule` (below) reads it off the generic entity-attribute channel.
// --- crossCheckSchema + applyChurnRule + analyzeSchema (usage branch) ---
pub const SKIP_FIELD_NAMES: &[&str] = &["id", "createdAt", "updatedAt"];

/// True when `name` appears at least once as an identifier in the scanned BE source.
fn identifier_seen(usage: &SchemaUsage, name: &str) -> bool {
    usage.identifier_counts.get(name).copied().unwrap_or(0) > 0
}

/// The Prisma client's delegate spelling for a model — the declared name with its first character
/// lowercased (`UserPassword` -> `userPassword`, matching `prisma.userPassword`) — but ONLY for a
/// multi-word name, and the restriction is the whole point.
///
/// `identifier_counts` is an unqualified whole-tree token bag: it records that the token `user` appeared,
/// not that `prisma.user` did. For a single-word model the delegate is therefore one of the most common
/// local-variable names in any TypeScript tree, and accepting it makes the rule VACUOUS rather than
/// merely loose — a review canary with `model User`/`model Team` and a source file containing nothing but
/// `const user = {n:1}; const team = {n:2};` turned two correct findings into two findings asserting the
/// opposite ("this model is used, this column is not"), because the model-level short-circuit stopped
/// firing and the field loop ran.
///
/// An internal uppercase is what makes the derived spelling distinctive, and it costs nothing measured:
/// all seven calcom/cal.com models this fix was built from are multi-word (`userPassword`, `hostGroup`,
/// `reminderMail`, three credit-ledger models, `videoCallGuest`), so the narrowing keeps 7/7 of the
/// benefit. **The single-word case stays unfixed and that is a stated miss**: a `model Booking` used only
/// as `prisma.booking` still reports. Closing it needs a RECEIVER-qualified token (`prisma.booking`),
/// which this substrate does not carry — a vacuous rule is a worse answer than a narrow one.
///
/// ASCII-only on purpose: Prisma model names are `[A-Za-z][A-Za-z0-9_]*`, so there is no locale question.
fn delegate_accessor(model_name: &str) -> Option<String> {
    let mut chars = model_name.chars();
    let first = chars.next()?;
    let rest = chars.as_str();
    if !rest.chars().any(|c| c.is_ascii_uppercase()) {
        return None;
    }
    Some(first.to_ascii_lowercase().to_string() + rest)
}

/// Schema cross-check — compares the schema-IR against actual BE code usage. Surfaces unreferenced-model-name (a model not bound to any store) and unreferenced-field-name (a field never appearing as an identifier in BE source)
/// issues. id/createdAt/updatedAt are excluded by default since infrastructure fields are rarely referenced directly.
pub fn cross_check_schema(
    models: &[SchemaModel],
    usage: &SchemaUsage,
    attrs: &AttributeStore,
    skip_field_names: &[&str],
) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    for model in models {
        // A model is "used" if its name appears as an identifier anywhere in BE source
        // (`identifier_counts`, the generic vocab-free signal — same substrate unreferenced-field-name uses), OR if a
        // Mode-B producer injected a truthy `BOUND_MODEL_ATTR` on the model's `Symbol` through the generic
        // entity-attribute channel. That channel is empty under native analysis now that the app-specific
        // store-binding recognizer is gone. This makes unreferenced-model-name a general "the model name is never
        // referenced" check instead of "the model isn't wired through one project's store convention."
        let referenced = identifier_seen(usage, &model.name)
            // The GENERATED CLIENT never spells the model name. Prisma lowercases the first letter to
            // build its delegate — a `model UserPassword` is reached as `prisma.userPassword.findUnique`
            // — so correct, heavily-used code contains the PascalCase name nowhere at all, and this rule
            // reported it as unreferenced. Measured on calcom/cal.com `176037d`: 7 of 7 findings examined
            // were this, every one of them a model in daily use (`prisma.userPassword`, `prisma.hostGroup`,
            // `prisma.reminderMail`, three credit-ledger models, `prisma.videoCallGuest`).
            //
            // The camelCase form is DERIVED from the model name, not a spelling anyone maintains, so a
            // model named tomorrow is covered without a row being added — which is what keeps this from
            // becoming the kind of hand list that leaves everything outside it invisible forever.
            // `delegate_accessor` returns None for a SINGLE-word name; its doc has the canary that
            // forced that restriction.
            || delegate_accessor(&model.name).is_some_and(|d| identifier_seen(usage, &d));
        let bound = attrs
            .symbol_attr(&model.name, None, BOUND_MODEL_ATTR)
            .is_some_and(zzop_core::attr_is_truthy);
        if !referenced && !bound {
            issues.push(SchemaIssue {
                rule: "unreferenced-model-name".to_string(),
                severity: Severity::Info,
                model: model.name.clone(),
                field: None,
                params: None,
            });
            continue;
        }
        // Every model name in this schema, for the relation-navigator test below.
        let model_names: std::collections::HashSet<&str> =
            models.iter().map(|m| m.name.as_str()).collect();
        for field in &model.fields {
            if skip_field_names.contains(&field.name.as_str()) {
                continue;
            }
            // NO MINIMUM NAME LENGTH — a `MIN_FIELD_NAME_LEN = 3` floor skipped names of one or two
            // characters until 2026-08-29, on the rationale that "very short field names appear
            // everywhere in BE source, so the detection is meaningless". Two things were wrong with it.
            //
            // It was a PROXY FOR A FACT THIS RULE ALREADY MEASURES. "Does this name appear in source"
            // is not something to approximate from the name's length — `identifier_counts` counts it
            // directly, three lines below. A short name that appears is dropped by that count; a short
            // name that genuinely never appears anywhere in the tree is exactly as strong a signal as a
            // long one, and the floor threw it away before the evidence was consulted.
            //
            // And it never fired. Measured 2026-08-29 over the corpus's only Prisma schema
            // (calcom/cal.com, 100 models): after `SKIP_FIELD_NAMES` and the navigator test below, the
            // candidate field-name length distribution starts at 3 — **zero** candidates of length 1 or
            // 2 exist. Two clean release builds one constant apart confirm it end to end: at 3 and at 2
            // the rule reports 27 findings, the same 27, and every other rule in all nine trees is
            // unmoved. A gate whose harvest is zero buys only risk (`.claude` rule-quality §26 (3)).
            // A RELATION NAVIGATOR is not a deletable field, so reporting it is never actionable — it is
            // the required opposite side of a `@relation` declared on the other model, and removing it
            // makes `prisma validate` fail outright: you cannot generate a client, let alone migrate.
            // Prisma also never requires code to name the back side (you traverse it through `include`),
            // so "no identifier hit" is the EXPECTED reading for a correct schema, not a signal.
            // Measured on calcom/cal.com `176037d`, where the advice was "remove the field": `Team.orgUsers`
            // (the back side of `User.organization @relation("scope")`), `Team.inviteTokens`,
            // `Team.accessCodes`, `Credential.CalendarCache` and more, out of 65 findings.
            //
            // The test is structural rather than attribute-based: a field whose declared TYPE is the name
            // of another model in this schema is a navigator. An enum-typed or scalar field is not, so a
            // genuinely dead column (`Team.hideBookATeamMember`, a real one in that same run) still reports.
            if model_names.contains(field.r#type.as_str()) {
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
                rule: "unreferenced-field-name".to_string(),
                severity: Severity::Info,
                model: model.name.clone(),
                field: Some(field.name.clone()),
                params: None,
            });
        }
    }
    issues
}

/// The churn line: report at or above `CHURN_REPORT_THRESHOLD`, in one band. **IT IS A CONVENTION AND
/// CANNOT BE MEASURED FROM HERE** — that second half is the whole reason this comment exists rather
/// than a number with a story.
///
/// There WAS a second line here (escalate to `critical` at 10), removed 2026-09-06 with the product
/// decision that moved this rule out of the bands a first screen is built from. Keeping it would have
/// left the loudest band in the tool being handed out by the threshold this very comment calls
/// unmeasurable — and the rule's own message tells the reader to judge the raw `data.count` against
/// their migration history instead, which the removal does not touch.
///
/// `apply_churn_rule` reads its count off an INJECTED attribute (`MODEL_CHURN_ATTR`), and no native
/// analysis in this repo produces one (see this module's header for why that recognizer was removed).
/// So the rule is silent by construction in every native run: measured 2026-08-29 across all nine
/// corpus trees, `schema/model-churn` reports **0** findings, and moving the line by one in either
/// direction moves nothing — 0 at 4, 0 at 5, 0 at 6. There is no population to calibrate against,
/// which means 5 cannot be justified by measurement and cannot be tuned by it either. It is a round
/// number, and the rule's message says so to whoever does have data.
///
/// The trigger for replacing it with a measurement is a Mode-B producer injecting real churn counts:
/// at that point the distribution exists, and the first person holding one should set this line from
/// it rather than inherit this one.
const CHURN_REPORT_THRESHOLD: u32 = 5;

/// model-churn rule (spelled `schema-churn` until 2026-08-02 — the id now carries the attribute's own name instead of repeating the pack name; old id in VERSIONING.md) — detects design instability from accumulated migration churn on a model. Churn count
/// per model is read off the generic entity-attribute channel (`MODEL_CHURN_ATTR` on the model's `Symbol`);
/// a model with no injected churn attribute is treated as zero, so this self-gates and is safe to always call.
pub fn apply_churn_rule(models: &[SchemaModel], attrs: &AttributeStore) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    for model in models {
        let count = attrs
            .symbol_attr(&model.name, None, MODEL_CHURN_ATTR)
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        if count < CHURN_REPORT_THRESHOLD {
            continue;
        }
        // Ships `info`, one band (2026-09-06). The message disowns this line in its own words -- "a
        // round number with no measurement behind it", "judge it against your own history rather than
        // against that line" -- so no tier here can carry a band that demands action. What the reader
        // judges is untouched: `data.count` still carries the raw count.
        issues.push(SchemaIssue {
            rule: "model-churn".to_string(),
            severity: Severity::Info,
            model: model.name.clone(),
            field: None,
            params: Some(serde_json::json!({ "count": count })),
        });
    }
    issues
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
    let mut extra = cross_check_schema(&analysis.models, &usage, attrs, SKIP_FIELD_NAMES);
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
