//! Human-facing message vocabulary for `SchemaIssue`s — one prose sentence per rule id, covering both the
//! structural rules (`structural.rs`) and the usage rules (`usage.rs`).

use crate::join::JoinIssue;
use crate::structural::SchemaIssue;
use zzop_core::disable_hint;

mod sightline;

use sightline::{field_usage_sightline, query_call_site_sightline};
pub use sightline::{rule_sightlines, QUERY_CALL_SITE_EXTENSIONS};
// The two pinned CLAIM fragments are used only by this module's seal tests (`tests.rs` reaches them
// through its `use super::*`), never by the message bodies — those splice the full sentences above.
#[cfg(test)]
use sightline::{field_usage_sightline_claim, query_call_site_sightline_claim};

/// `disable_hint`'s own fragment minus its leading `"Disable "` word — every message in this file that
/// embeds the disable hint mid-sentence (rather than as its own "Disable via config ..." sentence, the
/// shape most other native rules use) splices this in after its own lead-in verb instead of hand-writing
/// the `` `rules: {...}` (embedders: `disabled_rules`) `` fragment again, so this file still has exactly
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
            "Model {} has {} fields — consider splitting it into smaller, more cohesive models.",
            issue.model,
            param("fieldCount").unwrap_or_default()
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
                    "Model {} has a creation timestamp but no updatedAt field — if this model supports \
                     updates, consider adding an `updatedAt` field (with `@updatedAt`) to track them; if it \
                     is append-only/immutable, no change is needed.",
                    issue.model
                )
            } else {
                format!(
                    "Model {} is missing timestamp field(s): {}.",
                    issue.model,
                    param("missing").unwrap_or_default()
                )
            }
        }
        "redundant-index" => format!(
            "Model {} field {field} has a redundant @@index — already covered by @id/@unique.",
            issue.model
        ),
        "float-money" => format!(
            "Model {} field {field} stores a monetary value as a lossy float type ({}) — use Decimal.",
            issue.model,
            param("type").unwrap_or_else(|| "Float".to_string())
        ),
        "stale-updated-at" => format!(
            "Model {} field {field} looks like an updatedAt timestamp but lacks @updatedAt — it will not auto-refresh on writes.",
            issue.model
        ),
        "temporal-as-string" => format!(
            "Model {} field {field} stores a date/time value as String — use DateTime instead.",
            issue.model
        ),
        "fk-no-index" => {
            let coverage = issue
                .params
                .as_ref()
                .and_then(|p| p.get("coverage"))
                .and_then(|v| v.as_str())
                .unwrap_or("none");
            if coverage == "non-leading" {
                let composite_cols = issue
                    .params
                    .as_ref()
                    .and_then(|p| p.get("compositeCols"))
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                format!(
                    "Model {} field {field} is a non-leading member of the composite ({composite_cols}) \
                     @@index/@@unique — it is only covered for queries that ALSO constrain the leading \
                     column(s) of that composite, not for queries filtering on {field} alone.",
                    issue.model
                )
            } else {
                format!(
                    "Model {} field {field} looks like a foreign key but has no @@index/@@unique — queries filtering on it will scan the table.",
                    issue.model
                )
            }
        }
        "nullable-fk" => format!(
            "Model {} field {field} is a nullable foreign key — confirm the optional relation is intentional.",
            issue.model
        ),
        "implicit-fk" => format!(
            "Model {} field {field} looks like a foreign key with no @relation — the relation is implicit/unmodeled.",
            issue.model
        ),
        "unreferenced-model-name" => format!(
            "Model {}'s name never appears as an identifier in source, and no `bound-model` attribute \
             was injected for it — the model may be unused. {}",
            issue.model,
            field_usage_sightline()
        ),
        "unreferenced-field-name" => format!(
            "Model {} field {field}'s name never appears as an identifier in source — the field may be \
             unused. {}",
            issue.model,
            field_usage_sightline()
        ),
        "schema-churn" => format!(
            "Model {} accumulated {} migration change(s) — the design may be unstable.",
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
        "soft-delete-bypass" => format!(
            "Model {} has a soft-delete marker field ({field}) but this {method}() call has no `{field}` \
             filter in its arguments — it may return soft-deleted rows. Add `{field}: null` (or your app's \
             not-deleted convention) to the `where` clause. Note: a Prisma middleware (`$use`) or `$extends` \
             client extension that injects this filter globally is invisible to this static check — if your \
             app relies on one, this rule will false-positive on every call site for the model. {} \
             To silence that, disable it \
             {} (this rule \
             has no inline suppression marker).",
            issue.model,
            query_call_site_sightline(),
            disable_hint_tail("soft-delete-bypass")
        ),
        "orderby-unindexed" => format!(
            "Model {} is ordered by `{field}` in this {method}() call, but {field} has no @id/@unique of its \
             own and is not the leading column of any @@index/@@unique — this sort likely forces a full \
             table scan or filesort as the table grows. Add `@@index([{field}])` to the schema (or make \
             {field} the leading column of an existing composite index). {} If this is intentional (e.g. a \
             small, bounded table), disable this finding {} \
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
