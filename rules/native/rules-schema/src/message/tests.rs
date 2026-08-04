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

/// Pins the exact byte shape of the two-id disable-hint tail (`issue_disable_hint`) — this is
/// the one native message dialect that does NOT read `disable_hint`'s own "Disable via config ..."
/// output verbatim (it splices `disable_hint_tail` into a differently-worded sentence instead, see
/// `issue_disable_hint`'s doc), so this regression pin exists specifically to catch a future edit that
/// breaks that splice. It also pins the honesty property the promotion of these labels to registered ids
/// bought: the sentence names the issue's OWN id first, because that id is now what
/// `disabledRules`/`severityOverrides` actually match on.
#[test]
fn god_model_message_ends_with_the_exact_own_id_and_family_disable_hint() {
    let i = issue(
        "god-model",
        None,
        Some(serde_json::json!({ "fieldCount": "40" })),
    );
    let msg = schema_issue_message(&i);
    assert!(
        msg.ends_with(
            " Disable this one rule via config `rules: { \"schema/god-model\": \"off\" }` (embedders: \
             `disabledRules`), or its whole family via config `rules: { \"schema-structural\": \
             \"off\" }` (embedders: `disabledRules`); to drop a single finding, use config `exclude` \
             (or a per-rule `exclude`) on its file path instead."
        ),
        "unexpected message tail: {msg:?}"
    );
}

/// Same pin as the structural test above, for the usage family.
#[test]
fn unreferenced_model_name_message_ends_with_the_exact_own_id_and_family_disable_hint() {
    let i = issue("unreferenced-model-name", None, None);
    let msg = schema_issue_message(&i);
    assert!(
        msg.ends_with(
            " Disable this one rule via config `rules: { \"schema/unreferenced-model-name\": \"off\" \
             }` (embedders: `disabledRules`), or its whole family via config `rules: { \
             \"schema-usage\": \"off\" }` (embedders: `disabledRules`); to drop a single finding, use \
             config `exclude` (or a per-rule `exclude`) on its file path instead."
        ),
        "unexpected message tail: {msg:?}"
    );
}

/// Every registered `schema/<label>` id is a label `schema_issue_message` RECOGNIZES: it renders a
/// specific body (not the `schema rule '<other>' fired.` fallback) and appends a hint naming that exact
/// id. The registration lists and the message dispatch read the same two constants, so this is the seal
/// that the third copy — the `rule:` literals `structural.rs`/`usage.rs` actually emit — has not drifted
/// from them: a label emitted under a name nobody registered would reach a user as a `ruleId` that
/// `disabledRules` cannot match, which is exactly the defect the promotion removed.
#[test]
fn every_registered_schema_issue_id_has_a_specific_message_naming_that_id() {
    for label in crate::SCHEMA_STRUCTURAL_ISSUE_LABELS
        .iter()
        .chain(crate::SCHEMA_USAGE_ISSUE_LABELS.iter())
    {
        let msg = schema_issue_message(&issue(label, Some("someField"), None));
        assert!(
            !msg.contains("fired."),
            "`{label}` fell through to the generic fallback message: {msg}"
        );
        let own = format!("\"{}\"", crate::schema_issue_rule_id(label));
        assert!(
            msg.contains(&own),
            "`{label}`'s message never names its own registered id {own}: {msg}"
        );
    }
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
             `disabledRules`) (this rule has no inline suppression marker)."
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
             `disabledRules`) (this rule has no inline suppression marker)."
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
             `disabledRules`) (this rule has no inline suppression marker)."
        ),
        "unexpected message tail: {msg:?}"
    );
}

/// The published pages that must carry this crate's two language sightlines. `docs/rules/catalog.md` is
/// additionally embedded in the shipped binary (`crates/summary/src/contracts.rs`'s `rule-catalog` resource),
/// so its copy is what an MCP client reads without a source checkout. `docs/getting-started.md` is NOT in
/// the list on purpose: it never names any of these five rule ids, so a sightline there would have nothing
/// to attach to — see this crate's report note.
const SIGHTLINE_PROSE_PAGES: [&str; 2] =
    ["../../../docs/rules/catalog.md", "../../../site/rules.html"];

/// Whitespace-collapsing containment check — Markdown and HTML both render a newline inside a paragraph
/// as a space, so a page that wraps the pinned sentence is byte-different but reader-identical. Compares
/// the WORDS (the policy) rather than forcing three prose files to keep one long line unwrapped.
fn page_says(rel: &str, claim: &str) -> bool {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read published page {}: {e}", path.display()));
    let collapse = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    collapse(&text).contains(&collapse(claim))
}

/// Policy pin (T2 — a Markdown/HTML page cannot reference a Rust constant): the three JOIN rules only ever
/// see a Prisma client called from TypeScript, because `QueryCallSite` has exactly one producer. The schema
/// half of the join IS language-neutral, so a `prisma-client-py`/`prisma-client-go` repo parses its schema
/// fully, extracts zero call sites, and reports zero findings — indistinguishable from clean. Nothing else
/// is red when a page forgets this: the JOIN tests assert firing behavior, never the sentence describing
/// where firing is possible at all.
#[test]
fn the_query_call_site_sightline_is_identical_in_the_findings_and_the_docs() {
    let claim = query_call_site_sightline_claim();
    for (rule, params) in [
        ("soft-delete-bypass", None),
        ("orderby-unindexed", None),
        (
            "enum-string-drift",
            Some(serde_json::json!({ "enum": "Status", "literal": "actve" })),
        ),
    ] {
        let msg = join_issue_message(&join_issue(rule, Some("status"), params));
        assert!(
            msg.contains(claim),
            "{rule}'s message no longer renders the shared sightline claim `{claim}`: {msg}"
        );
    }
    for rel in SIGHTLINE_PROSE_PAGES {
        assert!(
            page_says(rel, claim),
            "{rel} no longer says `{claim}` — a reader whose Prisma client is Python or Go is told \
             nothing about why these three rules report zero, which is the false assurance this \
             sightline exists to prevent"
        );
    }
}

/// Policy pin (T2), same shape, for the INVERTED case: `unreferenced-model-name`/`unreferenced-field-name` do not go silent when
/// their evidence channel is empty — they ASSERT. `field_usage_tokens` only ever scans
/// `FIELD_USAGE_SCAN_EXTENSIONS`, so a tree holding a schema and no `.ts`/`.tsx` supplies zero identifier
/// evidence and every model reports dead (measured 2026-07-25 on a directory containing one
/// `schema.prisma`: 2 models in, 2 `unreferenced-model-name` findings out). The claim is derived from the constant, so
/// the extension list cannot drift out of the published sightline.
#[test]
fn the_field_usage_sightline_is_identical_in_the_findings_and_the_docs() {
    let claim = field_usage_sightline_claim();
    for rule in ["unreferenced-model-name", "unreferenced-field-name"] {
        let msg = schema_issue_message(&issue(rule, Some("someField"), None));
        assert!(
            msg.contains(&claim),
            "{rule}'s message no longer renders the shared sightline claim `{claim}`: {msg}"
        );
    }
    for rel in SIGHTLINE_PROSE_PAGES {
        assert!(
            page_says(rel, &claim),
            "{rel} no longer says `{claim}` — a reader looking at a schema-only tree (a topology this \
             project's own multi-tree advice recommends) is told every model is dead with no hint \
             that nothing was searched"
        );
    }
}
