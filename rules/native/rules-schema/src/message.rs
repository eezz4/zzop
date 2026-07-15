//! Human-facing message vocabulary for `SchemaIssue`s — one prose sentence per rule id, covering both the
//! structural rules (`structural.rs`) and the usage rules (`usage.rs`).

use crate::join::JoinIssue;
use crate::structural::SchemaIssue;
use zzop_core::disable_hint;

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

/// Builds the "whole family" disable-hint sentence appended to every structural/usage message
/// (`schema_structural_disable_hint`/`schema_usage_disable_hint` below): reworded around the fact that
/// these two gate ids disable a whole rule FAMILY, not one finding.
fn family_disable_hint(gate_id: &str) -> String {
    let tail = disable_hint_tail(gate_id);
    format!(
        " Disable the whole family {tail}; to drop just this one finding, use config `exclude` (or a \
         per-rule `exclude`) on its file path instead."
    )
}

/// `structural.rs`'s issue ids (`god-model`, `missing-timestamps`, `redundant-index`, `float-money`,
/// `stale-updated-at`, `temporal-as-string`, `fk-no-index`, `nullable-fk`, `implicit-fk`) are NOT
/// individually disableable — they're gated as one family behind the native analysis id
/// `"schema-structural"` (`crates/engine/src/pipeline.rs`'s `schema_findings`). Appended to every
/// structural message by `schema_issue_message`.
fn schema_structural_disable_hint() -> String {
    family_disable_hint("schema-structural")
}

/// `usage.rs`'s issue ids (`dead-model`, `dead-field`, `schema-churn`) are gated as one family behind the
/// native analysis id `"schema-usage"` (`crates/engine/src/pipeline.rs`'s `schema_usage_findings`,
/// `crates/engine/src/analyze/mod.rs`'s `is_enabled(&config.rule_config, "schema-usage")` call site).
/// Appended to every usage message by `schema_issue_message`.
fn schema_usage_disable_hint() -> String {
    family_disable_hint("schema-usage")
}

/// `SchemaIssue` itself carries no message — this is the one place that prose is authored. Falls back to a
/// generic (still informative) message for any rule id not recognized below, so an unmatched `issue.rule`
/// never panics. Every structural/usage message ends with a disable hint naming the REAL gate — `god-model`,
/// `fk-no-index`, `dead-model`, etc. are not disableable ids of their own (see the two hint constants' docs).
pub fn schema_issue_message(issue: &SchemaIssue) -> String {
    let field = issue.field.as_deref().unwrap_or("?");
    let param = |key: &str| -> Option<String> {
        issue
            .params
            .as_ref()
            .and_then(|p| p.get(key))
            .map(|v| v.to_string())
    };
    let hint = match issue.rule.as_str() {
        "god-model" | "missing-timestamps" | "redundant-index" | "float-money"
        | "stale-updated-at" | "temporal-as-string" | "fk-no-index" | "nullable-fk"
        | "implicit-fk" => schema_structural_disable_hint(),
        "dead-model" | "dead-field" | "schema-churn" => schema_usage_disable_hint(),
        _ => String::new(),
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
        "dead-model" => format!(
            "Model {} is not bound to any store/repository in code — it may be dead schema.",
            issue.model
        ),
        "dead-field" => format!(
            "Model {} field {field} never appears as an identifier in source — it may be dead schema.",
            issue.model
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
/// since native rules carry no inline suppression marker.
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
             app relies on one, this rule will false-positive on every call site for the model; disable it \
             {} (native \
             rules have no inline suppression marker).",
            issue.model,
            disable_hint_tail("soft-delete-bypass")
        ),
        "orderby-unindexed" => format!(
            "Model {} is ordered by `{field}` in this {method}() call, but {field} has no @id/@unique of its \
             own and is not the leading column of any @@index/@@unique — this sort likely forces a full \
             table scan or filesort as the table grows. Add `@@index([{field}])` to the schema (or make \
             {field} the leading column of an existing composite index). If this is intentional (e.g. a \
             small, bounded table), disable this finding {} \
             (native rules have no inline suppression marker).",
            issue.model,
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
                 array, a variable, or a computed expression is not. If this literal is intentional, disable \
                 this finding {} \
                 (native rules have no inline suppression marker).",
                issue.model,
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
mod tests {
    //! Regression coverage for the fk-no-index composite-coverage wording and the missing-timestamps
    //! append-only-model wording, using hand-built `SchemaIssue`s directly.
    use super::*;
    use zzop_core::Severity;

    fn issue(rule: &str, field: Option<&str>, params: Option<serde_json::Value>) -> SchemaIssue {
        SchemaIssue {
            rule: rule.to_string(),
            severity: Severity::Info,
            model: "M".to_string(),
            field: field.map(str::to_string),
            params,
        }
    }

    /// Pins the exact byte shape of the "whole family" disable-hint tail (`family_disable_hint`) — this is
    /// the one native message dialect that does NOT read `disable_hint`'s own "Disable via config ..."
    /// output verbatim (it splices `disable_hint_tail` into a differently-worded sentence instead, see
    /// `family_disable_hint`'s doc), so this regression pin exists specifically to catch a future edit that
    /// breaks that splice.
    #[test]
    fn god_model_message_ends_with_the_exact_whole_family_disable_hint() {
        let i = issue(
            "god-model",
            None,
            Some(serde_json::json!({ "fieldCount": "40" })),
        );
        let msg = schema_issue_message(&i);
        assert!(
            msg.ends_with(
                " Disable the whole family via config `rules: { \"schema-structural\": \"off\" }` \
                 (embedders: `disabled_rules`); to drop just this one finding, use config `exclude` (or a \
                 per-rule `exclude`) on its file path instead."
            ),
            "unexpected message tail: {msg:?}"
        );
    }

    /// Same pin as the structural test above, for the usage-family gate id.
    #[test]
    fn dead_model_message_ends_with_the_exact_whole_family_disable_hint() {
        let i = issue("dead-model", None, None);
        let msg = schema_issue_message(&i);
        assert!(
            msg.ends_with(
                " Disable the whole family via config `rules: { \"schema-usage\": \"off\" }` (embedders: \
                 `disabled_rules`); to drop just this one finding, use config `exclude` (or a per-rule \
                 `exclude`) on its file path instead."
            ),
            "unexpected message tail: {msg:?}"
        );
    }

    #[test]
    fn fk_no_index_none_coverage_message_unchanged() {
        let i = issue("fk-no-index", Some("ownerId"), None);
        let msg = schema_issue_message(&i);
        assert!(msg.contains("has no @@index/@@unique"));

        let i2 = issue(
            "fk-no-index",
            Some("ownerId"),
            Some(serde_json::json!({ "coverage": "none" })),
        );
        assert!(schema_issue_message(&i2).contains("has no @@index/@@unique"));
    }

    #[test]
    fn fk_no_index_non_leading_message_names_the_composite() {
        let i = issue(
            "fk-no-index",
            Some("guildId"),
            Some(serde_json::json!({
                "coverage": "non-leading",
                "compositeCols": ["a", "guildId"],
                "compositeKind": "unique",
            })),
        );
        let msg = schema_issue_message(&i);
        assert!(!msg.contains("has no @@index/@@unique"));
        assert!(msg.contains("a, guildId"));
        assert!(msg.contains("leading"));
    }

    #[test]
    fn missing_timestamps_updated_at_only_is_a_suggestion() {
        let i = issue(
            "missing-timestamps",
            None,
            Some(serde_json::json!({ "missing": ["updatedAt"] })),
        );
        let msg = schema_issue_message(&i);
        assert!(!msg.starts_with("Model M is missing timestamp field(s)"));
        assert!(msg.contains("if") || msg.contains("consider") || msg.contains("supports updates"));
    }

    #[test]
    fn missing_timestamps_created_at_missing_keeps_flatter_wording() {
        let i = issue(
            "missing-timestamps",
            None,
            Some(serde_json::json!({ "missing": ["createdAt", "updatedAt"] })),
        );
        let msg = schema_issue_message(&i);
        assert!(msg.starts_with("Model M is missing timestamp field(s)"));
    }

    // -----------------------------------------------------------------------------------------
    // join_issue_message disable-hint pins — same "splices disable_hint_tail mid-sentence" shape
    // as the family hints above, regression-pinned for the same reason.
    // -----------------------------------------------------------------------------------------

    fn join_issue(rule: &str, field: Option<&str>, params: Option<serde_json::Value>) -> JoinIssue {
        JoinIssue {
            rule: rule.to_string(),
            severity: Severity::Info,
            model: "M".to_string(),
            field: field.map(str::to_string),
            file: "schema.prisma".to_string(),
            line: 1,
            params,
        }
    }

    #[test]
    fn soft_delete_bypass_message_ends_with_the_exact_disable_hint() {
        let i = join_issue("soft-delete-bypass", Some("deletedAt"), None);
        let msg = join_issue_message(&i);
        assert!(
            msg.ends_with(
                "disable it via config `rules: { \"soft-delete-bypass\": \"off\" }` (embedders: \
                 `disabled_rules`) (native rules have no inline suppression marker)."
            ),
            "unexpected message tail: {msg:?}"
        );
    }

    #[test]
    fn orderby_unindexed_message_ends_with_the_exact_disable_hint() {
        let i = join_issue("orderby-unindexed", Some("createdAt"), None);
        let msg = join_issue_message(&i);
        assert!(
            msg.ends_with(
                "disable this finding via config `rules: { \"orderby-unindexed\": \"off\" }` (embedders: \
                 `disabled_rules`) (native rules have no inline suppression marker)."
            ),
            "unexpected message tail: {msg:?}"
        );
    }

    #[test]
    fn enum_string_drift_message_ends_with_the_exact_disable_hint() {
        let i = join_issue(
            "enum-string-drift",
            Some("status"),
            Some(serde_json::json!({ "enum": "Status", "literal": "actve" })),
        );
        let msg = join_issue_message(&i);
        assert!(
            msg.ends_with(
                "disable this finding via config `rules: { \"enum-string-drift\": \"off\" }` (embedders: \
                 `disabled_rules`) (native rules have no inline suppression marker)."
            ),
            "unexpected message tail: {msg:?}"
        );
    }
}
