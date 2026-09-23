//! Human-facing message vocabulary for `SchemaIssue`s — one prose sentence per rule id, covering both the
//! structural rules (`structural.rs`) and the usage rules (`usage.rs`).

use crate::join::JoinIssue;
use crate::structural::{god_model_threshold_claim, SchemaIssue, GOD_MODEL_MEASUREMENT};
use zzop_core::disable_hint;

mod sightline;

use sightline::{field_usage_sightline, query_call_site_sightline};

mod landing;

use landing::{
    DATA_LOSS_LANDING, FIELD_RETIREMENT_EXIT, FK_CONSTRAINT_LANDING, MIGRATION_LANDING,
    MODEL_RETIREMENT_EXIT, NOT_NULL_LANDING, NULLABLE_FK_EXIT,
};

mod index_build;

// The SILENT axis (no DDL, no error, delivered later) — a third sibling of
// `landing`/`index_build`; each module header is a promise the next author reuses.
mod silent_breakage;

use index_build::{CONCURRENT_INDEX_EXIT, INDEX_BUILD_LANDING};
pub use sightline::{rule_sightlines, QUERY_CALL_SITE_EXTENSIONS};
// The two pinned CLAIM fragments are used only by this module's seal tests (`tests.rs` reaches them
// through its `use super::*`), never by the message bodies — those splice the full sentences above.
#[cfg(test)]
use sightline::{field_usage_sightline_claim, query_call_site_sightline_claim};

/// `disable_hint`'s own fragment minus its leading `"Disable "` word — every message in this file that
/// embeds the disable hint mid-sentence (rather than as its own "Disable via config ..." sentence, the
/// shape most other native rules use) splices this in after its own lead-in verb instead of hand-writing
/// the `` `rules: {...}` (embedders: `disabledRules`) `` fragment again, so this file still has exactly
/// one source of truth for that fragment even though none of its call sites use `disable_hint`'s output
/// verbatim.
fn disable_hint_tail(id: &str) -> String {
    disable_hint(id)
        .strip_prefix("Disable ")
        .expect("disable_hint always starts with \"Disable \"")
        .to_string()
}

/// Builds the disable-hint sentence appended to every structural/usage message. Two ids, because there are
/// two real knobs: the issue's OWN registered id (`schema/god-model`, ... — see
/// [`crate::schema_issue_rule_id`]; honored by `is_enabled` at both schema call sites and by
/// `apply_severity_override` on the finding) and the FAMILY gate that switches the whole pass off. Naming
/// only the family — which is what this sentence did while the issue ids were unregistered labels — now
/// understates what the config accepts.
fn issue_disable_hint(label: &str, gate_id: &str) -> String {
    let own = disable_hint_tail(&crate::schema_issue_rule_id(label));
    let family = disable_hint_tail(gate_id);
    format!(
        " Disable this one rule {own}, or its whole family {family}; to drop a single finding, use config \
         `exclude` (or a per-rule `exclude`) on its file path instead."
    )
}

/// `structural.rs`'s issue ids ([`crate::SCHEMA_STRUCTURAL_ISSUE_LABELS`]) each report under
/// `schema/<label>` and are additionally gated as one family behind the native analysis id
/// `"schema-structural"` (`crates/engine/src/pipeline.rs`'s `schema_findings`). Appended to every
/// structural message by `schema_issue_message`.
fn schema_structural_disable_hint(label: &str) -> String {
    issue_disable_hint(label, "schema-structural")
}

/// `usage.rs`'s issue ids ([`crate::SCHEMA_USAGE_ISSUE_LABELS`]) each report under `schema/<label>` and are
/// additionally gated as one family behind the native analysis id `"schema-usage"`
/// (`crates/engine/src/pipeline.rs`'s `schema_usage_findings`, `crates/engine/src/analyze/mod.rs`'s
/// `is_enabled(&config.rule_config, "schema-usage")` call site). Appended to every usage message by
/// `schema_issue_message`.
fn schema_usage_disable_hint(label: &str) -> String {
    issue_disable_hint(label, "schema-usage")
}

/// `SchemaIssue` itself carries no message — this is the one place that prose is authored. Falls back to a
/// generic (still informative) message for any rule id not recognized below, so an unmatched `issue.rule`
/// never panics. Every structural/usage message ends with a disable hint naming BOTH the issue's own
/// registered id (`schema/god-model`, ...) and its family gate. Family membership is read off
/// [`crate::SCHEMA_STRUCTURAL_ISSUE_LABELS`]/[`crate::SCHEMA_USAGE_ISSUE_LABELS`] — the same two lists
/// `register_native_analyses` registers from, so a label can neither be registered without a message nor
/// carry a message without being registered.
pub fn schema_issue_message(issue: &SchemaIssue) -> String {
    let field = issue.field.as_deref().unwrap_or("?");
    let param = |key: &str| -> Option<String> {
        issue
            .params
            .as_ref()
            .and_then(|p| p.get(key))
            .map(|v| v.to_string())
    };
    let label = issue.rule.as_str();
    let hint = if crate::SCHEMA_STRUCTURAL_ISSUE_LABELS.contains(&label) {
        schema_structural_disable_hint(label)
    } else if crate::SCHEMA_USAGE_ISSUE_LABELS.contains(&label) {
        schema_usage_disable_hint(label)
    } else {
        String::new()
    };
    let body = match issue.rule.as_str() {
        "god-model" => format!(
            "Model {model} has {} fields — wide enough that its concerns are probably separable. \
             THIS RULE REPORTS ANY MODEL WITH {claim}, AND THAT NUMBER IS A CONVENTION, NOT A \
             MEASUREMENT: {GOD_MODEL_MEASUREMENT}, and the only real gap in that schema's \
             field-count distribution sits far higher (26 to 35). So read this as `wider than a \
             line someone drew`, not as `wider than comparable models`. \
             {DATA_LOSS_LANDING} IF YOU SPLIT THIS MODEL: add the new models, then hand-edit the \
             generated migration so it copies the rows across (`INSERT INTO \"New\" (...) SELECT \
             ... FROM \"{model}\"`) BEFORE the `DROP COLUMN` statements Prisma appended, and deploy \
             that as ONE migration; if you are not prepared to write that copy, leave the model as \
             it is — field count is a design opinion, and no reading of it is worth the rows.",
            param("fieldCount").unwrap_or_default(),
            claim = god_model_threshold_claim(),
            model = issue.model
        ),
        "missing-timestamps" => {
            let missing: Vec<String> = issue
                .params
                .as_ref()
                .and_then(|p| p.get("missing"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if missing.len() == 1 && missing[0] == "updatedAt" {
                // A creation timestamp already exists, and an append-only/immutable model legitimately
                // never gets an `updatedAt` — so this reads as a suggestion, not a defect claim.
                format!(
                    "Model {} has a creation timestamp but no updatedAt field. {MIGRATION_LANDING} IF \
                     THIS MODEL SUPPORTS UPDATES: add an `updatedAt` field and ship it together with \
                     that migration; if it is append-only/immutable, no change is needed.",
                    issue.model
                )
            } else {
                format!(
                    "Model {} is missing timestamp field(s): {}. {MIGRATION_LANDING} IF THESE FIELDS \
                     BELONG ON THIS MODEL: add `createdAt DateTime @default(now())`, which generates \
                     its own default and deploys as-is (it stamps every existing row with the \
                     migration's time, not the row's real creation time), and/or an `updatedAt` \
                     field, which does not and needs one of the two shapes above; if the model is \
                     append-only/immutable, it needs no `updatedAt`.",
                    issue.model,
                    param("missing").unwrap_or_default()
                )
            }
        }
        "redundant-index" => format!(
            "Model {} field {field} has a redundant @@index — already covered by @id/@unique.",
            issue.model
        ),
        "float-money" => landing::float_money_message(
            &issue.model,
            field,
            &param("type").unwrap_or_else(|| "Float".to_string()),
        ),
        // The SILENT axis: `@updatedAt` emits no DDL, so this edit is absent from the migration review
        // that would otherwise catch it (`silent_breakage`).
        "stale-updated-at" => silent_breakage::stale_updated_at_message(&issue.model, field),
        "temporal-as-string" => landing::temporal_as_string_message(&issue.model, field),
        "fk-no-index" => {
            index_build::fk_no_index_message(&issue.model, field, issue.params.as_ref())
        }
        "nullable-fk" => format!(
            "Model {} field {field} is a nullable foreign key — a declared `@relation` names it, so the \
             optional side is a choice this schema made. {NOT_NULL_LANDING} {NULLABLE_FK_EXIT}",
            issue.model
        ),
        "implicit-fk" => format!(
            "Model {} field {field} looks like a foreign key with no @relation — the relation is \
             implicit/unmodeled. {FK_CONSTRAINT_LANDING} IF EVERY EXISTING VALUE NAMES A REAL PARENT \
             ROW: add `@relation(fields: [{field}], references: [id])`. Otherwise backfill the \
             parentless values to NULL and make the column optional before adding it, or leave the \
             relation unmodeled — an id this database does not issue has no relation to model.",
            issue.model
        ),
        "unreferenced-model-name" => format!(
            "Model {}'s name never appears as an identifier in source, and no `bound-model` attribute \
             was injected for it — the model may be unused. {} {DATA_LOSS_LANDING} \
             {MODEL_RETIREMENT_EXIT}",
            issue.model,
            field_usage_sightline()
        ),
        "unreferenced-field-name" => format!(
            "Model {} field {field}'s name never appears as an identifier in source — the field may be \
             unused. {} {DATA_LOSS_LANDING} {FIELD_RETIREMENT_EXIT}",
            issue.model,
            field_usage_sightline()
        ),
        "model-churn" => format!(
            "Model {} accumulated {} migration change(s) — the design may be unstable. THAT COUNT IS \
             NOT ZZOP'S OWN: no native analysis produces migration churn, so it reached this rule from \
             a producer that knows this project's migration layout and injected it. The line the count \
             is read against — report at 5 — is a round number with no measurement behind it, because \
             zzop has never held a churn distribution to calibrate from, and for that same reason this rule \
             reports in ONE band only. The raw count is in `data.count`: judge it against your own history \
             rather than against that line.",
            issue.model,
            param("count").unwrap_or_default()
        ),
        other => format!(
            "Model {} field {field}: schema rule '{other}' fired.",
            issue.model
        ),
    };
    format!("{body}{hint}")
}

/// Message vocabulary for `join::JoinIssue` — JOIN rules anchored at a query call site rather than a model
/// declaration (see `join`'s module doc). Each message states the problem, the fix, and how to disable it,
/// since these rules carry no inline suppression marker (a blanket "no native rule does" would be false —
/// `zzop_rules_http`'s `non-idempotent-write`/`unsafe-read-endpoint` honor a hand-written `// idempotent-ok:`).
pub fn join_issue_message(issue: &JoinIssue) -> String {
    let field = issue.field.as_deref().unwrap_or("?");
    let method = issue
        .params
        .as_ref()
        .and_then(|p| p.get("method"))
        .and_then(|v| v.as_str())
        .unwrap_or("query");
    match issue.rule.as_str() {
        // Same axis: adding the filter emits nothing and raises nothing -- the call sites that EXIST to
        // read deleted rows just start returning none (`silent_breakage`).
        "soft-delete-bypass" => silent_breakage::soft_delete_bypass_message(
            &issue.model,
            field,
            method,
            &disable_hint_tail("soft-delete-bypass"),
        ),
        // Shares `INDEX_BUILD_LANDING`/`CONCURRENT_INDEX_EXIT` byte-identically with `fk-no-index` (the
        // mechanism belongs to the DDL, not to what either rule detects), and deliberately NOT
        // `FK_INDEX_NO_READER_EXIT`: this finding is anchored at a call site that already sorts on the
        // column, so "check that something reads it" is a question its own evidence has answered
        // (rule-quality.md §30). The bare imperative that used to sit here now carries its own condition
        // and follows the landing (§27).
        "orderby-unindexed" => format!(
            "Model {} is ordered by `{field}` in this {method}() call, but {field} has no @id/@unique of its \
             own and is not the leading column of any @@index/@@unique — this sort likely forces a full \
             table scan or filesort as the table grows. {INDEX_BUILD_LANDING} IF THAT COUNT IS SMALL: add \
             `@@index([{field}])` to the schema (or make {field} the leading column of an existing \
             composite index) and ship the generated migration as-is. {CONCURRENT_INDEX_EXIT} {} If this \
             is intentional (e.g. a small, bounded table), disable this finding {} \
             (this rule has no inline suppression marker).",
            issue.model,
            query_call_site_sightline(),
            disable_hint_tail("orderby-unindexed")
        ),
        "enum-string-drift" => {
            let enum_name = issue
                .params
                .as_ref()
                .and_then(|p| p.get("enum"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let literal = issue
                .params
                .as_ref()
                .and_then(|p| p.get("literal"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!(
                "Model {} field {field} is typed as the {enum_name} enum, but this {method}() call passes \
                 the string literal '{literal}', which is not one of {enum_name}'s declared members — likely \
                 a typo or a stale value left behind after the enum changed. Use one of {enum_name}'s \
                 members instead (the generated Prisma client's TS types would catch this at compile time, \
                 but a raw string literal — or a plain-JS caller — bypasses that check). Precision note: only \
                 a direct `{field}: '...'` literal-object site is checked; a literal inside an `in: [...]` \
                 array, a variable, or a computed expression is not. {} If this literal is intentional, disable \
                 this finding {} \
                 (this rule has no inline suppression marker).",
                issue.model,
                query_call_site_sightline(),
                disable_hint_tail("enum-string-drift")
            )
        }
        other => format!(
            "Model {} field {field}: schema-join rule '{other}' fired.",
            issue.model
        ),
    }
}

#[cfg(test)]
mod tests;
