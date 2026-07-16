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
