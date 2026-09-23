//! Regression coverage for the fk-no-index composite-coverage wording and the missing-timestamps
//! append-only-model wording, using hand-built `SchemaIssue`s directly.
use super::index_build::FK_INDEX_NO_READER_EXIT;
use super::landing::COLUMN_TYPE_CHANGE_LANDING;
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

/// POSITION, not presence (rule-quality.md §27). Both arms of this rule prescribe an edit whose
/// LANDING is the whole risk: a required `@updatedAt` column carries no default, so the migration
/// Prisma generates for it fails `prisma migrate deploy` against any table that already holds rows.
/// A reader who acts on the first instruction never reaches a caveat placed after it, so the landing
/// clause has to arrive BEFORE the imperative in EVERY arm.
///
/// The `starts_with` pins above are everything this file asserted until 2026-08-26, and they stay
/// GREEN with no landing clause at all -- which is how the both-missing arm shipped a message one
/// clause long ("Model Payment is missing timestamp field(s): [...]") while the updatedAt-only arm
/// carried prose. The invalidation probe for the assertions below is to move `MIGRATION_LANDING`
/// behind its arm's imperative: every token stays present and spelled exactly once, and this test
/// must go red on order alone.
fn assert_migration_landing_precedes(msg: &str, imperative: &str) {
    assert_eq!(
        msg.matches(MIGRATION_LANDING).count(),
        1,
        "the landing clause must be spelled ONCE, or an index comparison means nothing: {msg}"
    );
    assert_eq!(
        msg.matches(imperative).count(),
        1,
        "the imperative must be spelled ONCE, or an index comparison means nothing: {msg}"
    );
    let landing = msg.find(MIGRATION_LANDING).expect("landing clause missing");
    let verb = msg.find(imperative).expect("imperative missing");
    assert!(
        landing < verb,
        "the landing clause is at {landing} and the imperative at {verb} -- a reader who acts on \
         the instruction never reaches the caveat behind it: {msg}"
    );
    // The facts the clause exists to carry. Presence, unlike order, is what a rewrite loses.
    for needle in [
        "prisma migrate deploy",
        "already holds rows",
        "DEFAULT NOW()",
        "DateTime? @updatedAt",
        "not a one-line schema edit",
    ] {
        assert!(
            msg.contains(needle),
            "landing clause lost {needle:?}: {msg}"
        );
    }
}

#[test]
fn missing_timestamps_updated_at_only_lands_the_migration_before_the_imperative() {
    let i = issue(
        "missing-timestamps",
        None,
        Some(serde_json::json!({ "missing": ["updatedAt"] })),
    );
    assert_migration_landing_precedes(&schema_issue_message(&i), "IF THIS MODEL SUPPORTS UPDATES:");
}

/// The arm that fired on cal.com's `Payment` (`packages/prisma/schema.prisma:1095`), where the whole
/// delivered message was one clause naming two field names. A prescription-breaking finding stands on
/// three legs and only leg 3 (the message does not warn) is removable by prose -- an arm with no
/// prose has leg 3 open by construction, because there is nowhere for the warning to be.
#[test]
fn missing_timestamps_both_missing_lands_the_migration_before_the_imperative() {
    let i = issue(
        "missing-timestamps",
        None,
        Some(serde_json::json!({ "missing": ["createdAt", "updatedAt"] })),
    );
    assert_migration_landing_precedes(
        &schema_issue_message(&i),
        "IF THESE FIELDS BELONG ON THIS MODEL:",
    );
}

/// POSITION, not presence (rule-quality.md §27) — the second rule in this file to need it, and the first
/// that had NO prescription at all before: the shipped message was one observation plus three ways to turn
/// the rule off, so leg 3 of a prescription-breaking finding ("the message does not warn") was open by
/// construction. Prisma offers exactly one way to model the relation this message names, so a reader who
/// acts writes `@relation(fields: [...], references: [id])`, and Postgres validates the FOREIGN KEY that
/// generates against the rows already in the table.
///
/// The invalidation probe: move `FK_CONSTRAINT_LANDING` behind the imperative. Every token below stays
/// present and spelled exactly once, and this test must go red on ORDER alone.
#[test]
fn implicit_fk_lands_the_constraint_before_the_imperative() {
    let msg = schema_issue_message(&issue("implicit-fk", Some("teamId"), None));
    let imperative = "add `@relation(fields: [teamId], references: [id])`";
    assert_eq!(
        msg.matches(FK_CONSTRAINT_LANDING).count(),
        1,
        "the landing clause must be spelled ONCE, or an index comparison means nothing: {msg}"
    );
    assert_eq!(
        msg.matches(imperative).count(),
        1,
        "the imperative must be spelled ONCE, or an index comparison means nothing: {msg}"
    );
    let landing = msg.find(FK_CONSTRAINT_LANDING).expect("landing missing");
    let verb = msg.find(imperative).expect("imperative missing");
    assert!(
        landing < verb,
        "the landing clause is at {landing} and the imperative at {verb} -- a reader who acts on \
         the instruction never reaches the caveat behind it: {msg}"
    );
    // The facts the clause exists to carry. Presence, unlike order, is what a rewrite loses.
    for needle in [
        "prisma migrate deploy",
        "already in the table",
        "no parent row",
        "not a one-line schema edit",
        "sentinel",
    ] {
        assert!(
            msg.contains(needle),
            "landing clause lost {needle:?}: {msg}"
        );
    }
    // The residue the GATE cannot reach is what this prose is for (rule-quality.md §30): a column whose
    // parentless values are written by application code, or issued by a system this database does not own.
    // Naming the gated case (a declared `@default(0)`) instead would address readers who no longer exist.
    assert!(msg.contains("backfill"), "no way out named: {msg}");
    assert!(
        msg.contains("leave the relation unmodeled"),
        "the second exit must stay named -- one prescription that costs more than the finding is how a \
         reader learns to ignore the rule: {msg}"
    );
}

/// POSITION, not presence (rule-quality.md §27) — the third rule in this file to need it, and the second
/// that shipped with NO prescription at all: the message was one observation ("confirm the optional
/// relation is intentional") plus how to turn the rule off, so a reader who answered "no, it was not
/// intentional" was left to discover `SET NOT NULL` from a failed deploy. Leg 3 of a prescription-breaking
/// finding was open by construction.
///
/// The invalidation probe: move `NOT_NULL_LANDING` behind the imperative. Every token asserted below stays
/// present and spelled exactly once, and this test must go red on ORDER alone.
#[test]
fn nullable_fk_lands_the_not_null_check_before_the_imperative() {
    let msg = schema_issue_message(&issue("nullable-fk", Some("ownerId"), None));
    let imperative = "IF THE COUNT IS ZERO: drop the `?`";
    assert_eq!(
        msg.matches(NOT_NULL_LANDING).count(),
        1,
        "the landing clause must be spelled ONCE, or an index comparison means nothing: {msg}"
    );
    assert_eq!(
        msg.matches(imperative).count(),
        1,
        "the imperative must be spelled ONCE, or an index comparison means nothing: {msg}"
    );
    let landing = msg.find(NOT_NULL_LANDING).expect("landing clause missing");
    let verb = msg.find(imperative).expect("imperative missing");
    assert!(
        landing < verb,
        "the landing clause is at {landing} and the imperative at {verb} -- a reader who acts on the \
         instruction never reaches the caveat behind it: {msg}"
    );
    // The facts the clause exists to carry. Presence, unlike order, is what a rewrite loses.
    for needle in [
        "IS NULL",
        "SET NOT NULL",
        "prisma migrate deploy",
        "every row already stored",
        "not a one-line schema edit",
    ] {
        assert!(
            msg.contains(needle),
            "landing clause lost {needle:?}: {msg}"
        );
    }
    // BOTH exits, and following either ends green (rule-quality.md §26ⓑ — one prescription that costs
    // more than the finding is how a reader learns to ignore the rule). The zero-NULL branch is not
    // "just drop the `?`": the implicit `onDelete` flips with optionality, so a promotion with no
    // declared action changes what a parent delete does at runtime.
    assert!(msg.contains("backfill"), "no way out named: {msg}");
    assert!(
        msg.contains("LEAVE IT OPTIONAL") && msg.contains("mutually exclusive parents"),
        "the second exit must stay named -- for this rule it is frequently the right answer: {msg}"
    );
    assert!(
        msg.contains("`Restrict` for a required one"),
        "the runtime half of the promotion must stay named: {msg}"
    );
    // §30 — the message describes the population the GATES still reach, never the ones they removed. A
    // name-only `*Id` guess is gone (the lead-in asserts the declared `@relation` rather than hedging
    // with "looks like"), and so is the reader who only had to be told the optional side was deliberate.
    // `SetNull` survives in the exit for the opposite reason: it is Prisma's IMPLICIT action for every
    // finding that remains, none of which declares one.
    for gone in [
        "looks like a foreign key",
        "confirm the optional relation is intentional",
    ] {
        assert!(
            !msg.contains(gone),
            "message addresses a reader the gate removed ({gone:?}): {msg}"
        );
    }
    assert!(
        !msg.contains("onDelete: SetNull"),
        "a DECLARED SetNull is exactly what gate 2 removed -- no reader here has one: {msg}"
    );
}

// -----------------------------------------------------------------------------------------
// DATA_LOSS_LANDING position pins — the three rules whose prescription REMOVES something from
// the schema (rule-quality.md §34's open axis: "the prescription deletes data", not "the
// prescription breaks code").
//
// These three shipped with leg 3 of a prescription-breaking finding open in two different ways.
// `god-model`'s whole message was 85 characters ending in "consider splitting it into smaller,
// more cohesive models" — nowhere to put a caveat. The two `unreferenced-*` rules carried ~700
// characters of EVIDENCE disclosure and still spent zero of it on what the edit costs when the
// evidence is right: the reader is told how to check whether the field is really unused, and
// never told that acting drops the column with no backfill and no way back.
//
// The invalidation probe for all three: move `DATA_LOSS_LANDING` behind that rule's imperative.
// Every token stays present and spelled exactly once, and the assertion must go red on ORDER.
// -----------------------------------------------------------------------------------------

/// Shared body of the three pins below — the same shape as `assert_migration_landing_precedes`
/// above, against the other constant.
fn assert_data_loss_landing_precedes(rule: &str, msg: &str, imperative: &str) {
    assert_eq!(
        msg.matches(DATA_LOSS_LANDING).count(),
        1,
        "schema/{rule}: the landing clause must be spelled ONCE, or an index comparison means \
         nothing: {msg}"
    );
    assert_eq!(
        msg.matches(imperative).count(),
        1,
        "schema/{rule}: the imperative must be spelled ONCE, or an index comparison means \
         nothing: {msg}"
    );
    let landing = msg.find(DATA_LOSS_LANDING).expect("landing clause missing");
    let verb = msg.find(imperative).expect("imperative missing");
    assert!(
        landing < verb,
        "schema/{rule}: the landing clause is at {landing} and the imperative at {verb} -- a reader \
         who acts on the instruction never reaches the caveat behind it: {msg}"
    );
    // The facts the clause exists to carry. Presence, unlike order, is what a rewrite loses.
    for needle in [
        "CHECK WHAT THIS ALREADY HOLDS FIRST",
        "prisma/migrations/",
        "DROP COLUMN",
        "DROP TABLE",
        "not a one-line edit",
        "puts the values back",
    ] {
        assert!(
            msg.contains(needle),
            "schema/{rule}: landing clause lost {needle:?}: {msg}"
        );
    }
}

/// `god-model` is the odd one of the three and shares the constant anyway: its implicit edit MOVES
/// fields rather than deleting them, so the DROP is a side effect of the split rather than its
/// point. The mechanism the clause states — Prisma diffs the schema and emits DDL with no data
/// movement — is identical either way, and the difference lives entirely in the imperative, which is
/// per-rule regardless. Note the observation had to give up its own verb: the shipped sentence ended
/// "consider splitting it into smaller, more cohesive models", which IS the imperative, so leaving
/// it there would have put the clause behind the instruction by construction.
#[test]
fn god_model_lands_the_data_loss_before_the_imperative() {
    let msg = schema_issue_message(&issue(
        "god-model",
        None,
        Some(serde_json::json!({ "fieldCount": 41 })),
    ));
    assert_data_loss_landing_precedes("god-model", &msg, "IF YOU SPLIT THIS MODEL:");
    assert!(
        msg.contains("INSERT INTO"),
        "the exit must name the copy the migration will not write for you: {msg}"
    );
    assert!(
        msg.contains("leave the model as it is"),
        "the second exit must stay named -- a prescription with no way out is one a reader learns to \
         ignore: {msg}"
    );
    assert!(
        msg.contains("41 fields"),
        "the observation must survive the rewrite: {msg}"
    );
}

#[test]
fn unreferenced_field_name_lands_the_data_loss_before_the_imperative() {
    let msg = schema_issue_message(&issue(
        "unreferenced-field-name",
        Some("hideBookATeamMember"),
        None,
    ));
    assert_data_loss_landing_precedes(
        "unreferenced-field-name",
        &msg,
        "IF THE CONSUMING CODE REALLY IS ABSENT:",
    );
    assert!(
        msg.contains("TWO releases"),
        "the staged exit must stay named: {msg}"
    );
    assert!(
        msg.contains("leave the field declared"),
        "the do-nothing exit must stay named: {msg}"
    );
    // The evidence sightline is a different disclosure and must not be displaced by this one.
    assert!(
        msg.contains("EVIDENCE SIGHTLINE"),
        "the evidence sightline left the message: {msg}"
    );
}

#[test]
fn unreferenced_model_name_lands_the_data_loss_before_the_imperative() {
    let msg = schema_issue_message(&issue("unreferenced-model-name", None, None));
    assert_data_loss_landing_precedes(
        "unreferenced-model-name",
        &msg,
        "IF THE CONSUMING CODE REALLY IS ABSENT:",
    );
    assert!(
        msg.contains("TWO releases"),
        "the staged exit must stay named: {msg}"
    );
    assert!(
        msg.contains("leave the model declared"),
        "the do-nothing exit must stay named: {msg}"
    );
    assert!(
        msg.contains("EVIDENCE SIGHTLINE"),
        "the evidence sightline left the message: {msg}"
    );
}
// -----------------------------------------------------------------------------------------
// COLUMN_TYPE_CHANGE_LANDING position pins — the two rules whose prescription changes a
// column's declared TYPE (rule-quality.md §27, and §34 for the shape: neither rule wrote a
// prescription at all. `float-money` ended "— use Decimal." and `temporal-as-string` ended
// "— use DateTime instead.", a total of two verbs and nowhere to put a caveat, which is
// exactly §34's "a message with no prescription has no room for the qualifier either").
//
// The invalidation probe for both: move `COLUMN_TYPE_CHANGE_LANDING` behind that rule's
// imperative. Every token stays present and spelled exactly once, and the assertion goes red
// on ORDER alone.
// -----------------------------------------------------------------------------------------

/// Shared body of the two pins below — the same shape as `assert_data_loss_landing_precedes`,
/// against the type-change constant.
fn assert_type_change_landing_precedes(rule: &str, msg: &str, imperative: &str) {
    assert_eq!(
        msg.matches(COLUMN_TYPE_CHANGE_LANDING).count(),
        1,
        "schema/{rule}: the landing clause must be spelled ONCE, or an index comparison means \
         nothing: {msg}"
    );
    assert_eq!(
        msg.matches(imperative).count(),
        1,
        "schema/{rule}: the imperative must be spelled ONCE, or an index comparison means \
         nothing: {msg}"
    );
    let landing = msg
        .find(COLUMN_TYPE_CHANGE_LANDING)
        .expect("landing clause missing");
    let verb = msg.find(imperative).expect("imperative missing");
    assert!(
        landing < verb,
        "schema/{rule}: the landing clause is at {landing} and the imperative at {verb} -- a reader \
         who acts on the instruction never reaches the caveat behind it: {msg}"
    );
    // The facts the clause exists to carry. Presence, unlike order, is what a rewrite loses.
    for needle in [
        "CHANGING A COLUMN'S TYPE IS A MIGRATION",
        "SET DATA TYPE",
        "ACCESS EXCLUSIVE",
        "REWRITING the entire table",
        "fails the statement mid-deploy",
    ] {
        assert!(
            msg.contains(needle),
            "schema/{rule}: landing clause lost {needle:?}: {msg}"
        );
    }
}

/// `float-money` is the arm whose cast EXISTS — but that is not the same as an arm that cannot fail,
/// which is what this exit claimed until 2026-08-31. A `numeric(p, s)` narrower than the stored
/// magnitudes reaches exactly the landing's "any stored value the cast cannot produce" death, so the
/// three needles below hold the cast's real context ('a', assignment), the error it raises, and the
/// unannotated width that avoids it. Both exits are pinned because a message that ends in "or leave
/// it" is the shape this batch is repairing: the batched-column form is the one a reader with a large
/// table actually needs.
#[test]
fn float_money_lands_the_type_change_before_the_imperative() {
    let msg = schema_issue_message(&issue(
        "float-money",
        Some("amount"),
        Some(serde_json::json!({ "type": "Float" })),
    ));
    assert_type_change_landing_precedes("float-money", &msg, "IF THAT COUNT IS SMALL:");
    for needle in [
        "ASSIGNMENT cast",
        "numeric field overflow",
        "DECIMAL(65,30)",
        "add a second `Decimal` field",
        "rounded when they were WRITTEN",
    ] {
        assert!(msg.contains(needle), "float-money lost {needle:?}: {msg}");
    }
}

/// `temporal-as-string` is the mirror arm: the cast the landing describes does not exist for this
/// pair at all, so the generated statement never reaches a row. The pin holds the ORDER of the two
/// failures as well as the order of clause and verb — a reader who fixes only the syntax meets the
/// unparseable-value failure on the next attempt, so `USING` has to arrive before that sentence.
#[test]
fn temporal_as_string_lands_the_type_change_before_the_imperative() {
    let msg = schema_issue_message(&issue("temporal-as-string", Some("startsAt"), None));
    assert_type_change_landing_precedes(
        "temporal-as-string",
        &msg,
        "Change the field to `DateTime`",
    );
    let using = msg
        .find("USING <column>::timestamptz")
        .expect("USING missing");
    let parse_failure = msg
        .find("aborts on the FIRST stored value")
        .expect("parse-failure sentence missing");
    assert!(
        using < parse_failure,
        "temporal-as-string: the data failure at {parse_failure} is named ahead of the syntax fix at \
         {using}, so a reader meets them in the wrong order: {msg}"
    );
    assert!(
        msg.contains("no implicit text-to-timestamp conversion"),
        "the missing-cast fact left the message: {msg}"
    );
}

// -----------------------------------------------------------------------------------------
// INDEX_BUILD_LANDING position pins — the AVAILABILITY axis (rule-quality.md §27 for the
// position, §34 for the shape). `fk-no-index` is the largest schema rule on the corpus and
// shipped a 124-character observation with a prescription of ZERO characters, so leg 3 of a
// prescription-breaking finding ("the message does not warn") was open by construction: an
// implicit imperative that is perfectly unambiguous (`@@index([field])`) and nowhere at all
// to put the qualifier.
//
// What this axis is NOT: a prescription-breaking finding. `CREATE INDEX` destroys no data,
// breaks no deploy, and leaves the code correct — an auditor built the finding on this rule
// and then killed it himself, because if that counted, every index rule in the world would
// qualify. What it costs is availability, and nobody had measured it.
//
// The invalidation probe for all three pins: move `INDEX_BUILD_LANDING` behind that arm's
// imperative. Every token stays present and spelled exactly once, and the assertion must go
// red on ORDER alone.
// -----------------------------------------------------------------------------------------

/// Shared body of the three pins below — same shape as `assert_data_loss_landing_precedes`, against
/// the availability constant. `who` names the arm so a failure says which of the three moved.
fn assert_index_build_landing_precedes(who: &str, msg: &str, imperative: &str) {
    for (name, needle) in [
        ("the landing clause", INDEX_BUILD_LANDING),
        ("the large-table exit", CONCURRENT_INDEX_EXIT),
        ("the imperative", imperative),
    ] {
        assert_eq!(
            msg.matches(needle).count(),
            1,
            "{who}: {name} must be spelled ONCE, or an index comparison means nothing: {msg}"
        );
    }
    let landing = msg.find(INDEX_BUILD_LANDING).expect("landing missing");
    let verb = msg.find(imperative).expect("imperative missing");
    let exit = msg.find(CONCURRENT_INDEX_EXIT).expect("exit missing");
    assert!(
        landing < verb,
        "{who}: the landing clause is at {landing} and the imperative at {verb} -- a reader who acts \
         on the instruction never reaches the caveat behind it: {msg}"
    );
    assert!(
        verb < exit,
        "{who}: the large-table exit at {exit} must FOLLOW the small-table imperative at {verb}, or \
         the two branches reach the reader in the wrong order: {msg}"
    );
    // The facts the clause exists to carry. Presence, unlike order, is what a rewrite loses. Each of
    // these is a claim this sentence can stand behind: Prisma's diff never emits the concurrent form,
    // a plain build holds SHARE (writes wait, reads do not), and the count is what prices the window.
    for needle in [
        "COUNT THE ROWS IN THIS TABLE FIRST",
        "never generates the concurrent form",
        "`SHARE` lock",
        "reads keep working",
        "your whole deploy window",
    ] {
        assert!(
            msg.contains(needle),
            "{who}: landing lost {needle:?}: {msg}"
        );
    }
    // The exit is a way out only if it names what CONCURRENTLY costs and what batching does. A
    // prescription that ends "use CONCURRENTLY" sends a reader into a transaction-block error.
    for needle in [
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS",
        "cannot run inside a transaction block",
        "INVALID index",
        "their SUM, not the longest of them",
    ] {
        assert!(msg.contains(needle), "{who}: exit lost {needle:?}: {msg}");
    }
}

/// The arm that fired on cal.com's `Host.groupId` — 77 of this rule's 95 findings, and the one whose
/// whole delivered body was 124 characters: one observation, no prescription, no qualifier.
#[test]
fn fk_no_index_none_arm_lands_the_index_build_before_the_imperative() {
    let msg = schema_issue_message(&issue("fk-no-index", Some("groupId"), None));
    assert_index_build_landing_precedes(
        "fk-no-index (none)",
        &msg,
        "IF SOMETHING DOES FILTER ON IT AND THAT COUNT IS SMALL: add `@@index([groupId])`",
    );
    assert!(
        msg.contains("looks like a foreign key but has no @@index/@@unique"),
        "the observation must survive the rewrite: {msg}"
    );
}

/// The `info` arm (18 of the 95). It prescribes the same edit against the same DDL, so it takes the
/// same two shared sentences — the split from the arm above is the observation and the fact that the
/// index asked for here is a SECOND one beside a composite that already names the column.
#[test]
fn fk_no_index_non_leading_arm_lands_the_index_build_before_the_imperative() {
    let msg = schema_issue_message(&issue(
        "fk-no-index",
        Some("guildId"),
        Some(serde_json::json!({
            "coverage": "non-leading",
            "compositeCols": ["a", "guildId"],
            "compositeKind": "unique",
        })),
    ));
    assert_index_build_landing_precedes(
        "fk-no-index (non-leading)",
        &msg,
        "add a SECOND index, `@@index([guildId])`",
    );
    assert!(
        msg.contains("a, guildId") && msg.contains("non-leading member of the composite"),
        "the observation must survive the rewrite: {msg}"
    );
}

/// `orderby-unindexed` takes the SAME two constants, byte-identically — the mechanism belongs to the
/// DDL rather than to what either rule detects, the same reason `DATA_LOSS_LANDING` covers three rules.
/// Unlike its sibling this rule already HAD an imperative; it was bare, and it now carries its own
/// condition and sits behind the landing.
#[test]
fn orderby_unindexed_lands_the_index_build_before_the_imperative() {
    let msg = join_issue_message(&join_issue("orderby-unindexed", Some("createdAt"), None));
    assert_index_build_landing_precedes(
        "orderby-unindexed",
        &msg,
        "IF THAT COUNT IS SMALL: add `@@index([createdAt])`",
    );
    assert!(
        !msg.contains("as the table grows. Add `@@index("),
        "the bare imperative is back ahead of the landing: {msg}"
    );
    // The sightline is a different disclosure and must not be displaced by this one.
    assert!(
        msg.contains("LANGUAGE SIGHTLINE"),
        "the language sightline left the message: {msg}"
    );
}

/// BYTE-IDENTITY, the property that makes one constant across two rules worth having: a second
/// spelling of either sentence would make every index comparison above meaningless, and would let the
/// two rules drift into describing the same `CREATE INDEX` differently. Asserted across all three
/// firing shapes at once rather than per-arm, because drift is only visible in the comparison.
#[test]
fn the_two_index_build_sentences_are_one_spelling_across_both_rules() {
    let msgs = [
        schema_issue_message(&issue("fk-no-index", Some("groupId"), None)),
        schema_issue_message(&issue(
            "fk-no-index",
            Some("guildId"),
            Some(serde_json::json!({
                "coverage": "non-leading",
                "compositeCols": ["a", "guildId"],
            })),
        )),
        join_issue_message(&join_issue("orderby-unindexed", Some("createdAt"), None)),
    ];
    for m in &msgs {
        assert!(
            m.contains(INDEX_BUILD_LANDING) && m.contains(CONCURRENT_INDEX_EXIT),
            "one of the three index messages does not splice both shared sentences: {m}"
        );
    }
}

/// §30, pinned as a REFUSAL: `orderby-unindexed` must NOT carry [`FK_INDEX_NO_READER_EXIT`].
///
/// Sharing it would look like consistency and would be wrong. That sentence tells a reader to check
/// whether any query reads the column — a question `fk-no-index` cannot answer (it reads the field
/// NAME) and `orderby-unindexed` has already answered (its anchor IS a call site sorting on that
/// column). Splicing it there hands a reader a sentence undoing what they were just told, the trap
/// `NOT_NULL_LANDING` avoids by not reusing `FK_CONSTRAINT_LANDING`. Without this pin the next author
/// to notice "the sibling has three sentences and this one has two" closes the gap the wrong way.
#[test]
fn the_no_reader_check_reaches_only_the_rule_that_cannot_answer_it() {
    // BOTH arms, and the params are what selects them -- a loop over two field NAMES with `None`
    // params renders the none-arm twice and leaves the composite arm unpinned. Found by the
    // invalidation probe for this very assertion, which stayed green with that arm's clauses swapped.
    for (arm, params) in [
        ("none", None),
        (
            "non-leading",
            Some(serde_json::json!({
                "coverage": "non-leading",
                "compositeCols": ["a", "guildId"],
            })),
        ),
    ] {
        let msg = schema_issue_message(&issue("fk-no-index", Some("guildId"), params));
        assert!(
            msg.contains(FK_INDEX_NO_READER_EXIT),
            "fk-no-index ({arm}) guesses from the NAME and must say so: {msg}"
        );
        let no_reader = msg.find(FK_INDEX_NO_READER_EXIT).expect("clause missing");
        let landing = msg.find(INDEX_BUILD_LANDING).expect("landing missing");
        assert!(
            no_reader < landing,
            "fk-no-index ({arm}): the no-reader check is at {no_reader}, behind the landing at \
             {landing} -- if nothing reads the column the table's size is irrelevant, so pricing the \
             build first prices a build that should not run: {msg}"
        );
    }
    let ob = join_issue_message(&join_issue("orderby-unindexed", Some("createdAt"), None));
    assert!(
        !ob.contains(FK_INDEX_NO_READER_EXIT),
        "orderby-unindexed is anchored at a call site that already sorts on this column -- telling \
         that reader to check whether anything reads it contradicts the finding itself: {ob}"
    );
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

/// Policy pin (T2 — a Markdown/HTML page cannot reference a Rust constant): `god-model`'s THRESHOLD
/// and the measurement that justifies it are one policy, spelled by hand in English prose on both
/// published pages.
///
/// Why it must be pinned and why nothing else was red: `GOD_THRESHOLD` decides this rule's entire
/// output, and the pages restated its value plus a four-number measurement (`27`/`32`/`23`, on a
/// 100-model schema) with no comparison anywhere. `scripts/check-rules-catalog-sync.sh` reads ids,
/// severities and matcher names — tokens comparable as sets — and deliberately not the prose, which
/// is where these numbers live. So moving the constant would have left two published pages, one of
/// them embedded in the shipped binary as the `rule-catalog` resource an MCP client reads without a
/// checkout, asserting a line the rule no longer draws.
///
/// The finding's own message is NOT a third copy: it renders `god_model_threshold_claim` and
/// `GOD_MODEL_MEASUREMENT` directly (same crate, so T1), and is asserted here only to prove the
/// pinned strings are the ones that actually ship.
///
/// The `GOD_THRESHOLD == 15` assertion is the RE-MEASURE tripwire, not a duplicate of the constant:
/// the measurement sentence names the threshold it was taken at, so moving the constant makes that
/// sentence a reading about a line nobody draws any more. Failing here is how that gets noticed —
/// re-measure, rewrite `GOD_MODEL_MEASUREMENT`, update both pages, then move this number.
#[test]
fn the_god_model_threshold_is_identical_in_the_constant_and_the_published_docs() {
    assert_eq!(
        crate::structural::GOD_THRESHOLD,
        15,
        "GOD_MODEL_MEASUREMENT was taken with the threshold at 15 and names that number in its own \
         text, so it does not travel with the constant. Re-measure on a Prisma schema, rewrite that \
         sentence, mirror it into docs/rules/catalog.md (then regenerate the site pages), and only \
         then update this assertion — see the constant's own doc for why interpolating the \
         threshold into the sentence would have been the dishonest repair."
    );

    let claim = crate::structural::god_model_threshold_claim();
    let msg = schema_issue_message(&issue("god-model", None, None));
    for part in [claim.as_str(), crate::structural::GOD_MODEL_MEASUREMENT] {
        assert!(
            msg.contains(part),
            "god-model's message no longer renders `{part}`, so the pages below would be pinned \
             against a string the finding does not ship: {msg}"
        );
    }
    for rel in SIGHTLINE_PROSE_PAGES {
        for part in [claim.as_str(), crate::structural::GOD_MODEL_MEASUREMENT] {
            assert!(
                page_says(rel, part),
                "{rel} no longer says `{part}` — a reader is then told this rule draws its line \
                 somewhere the code does not, on the one number that decides its entire output"
            );
        }
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

// -----------------------------------------------------------------------------------------
// The SILENT axis (`message/silent_breakage.rs`) -- §27 ORDER pins. Neither prescription emits
// DDL and neither raises an error, so the cost is delivered later to a code path that stops
// getting what it used to. Position, not existence: the invalidation probe for both moves the
// landing behind that arm's imperative and leaves every token present.
// -----------------------------------------------------------------------------------------

/// Shared body of the two pins below.
fn assert_silent_landing_order(
    who: &str,
    msg: &str,
    disqualifier: &str,
    landing: &str,
    imperative: &str,
) {
    for (name, needle) in [
        ("the disqualifier", disqualifier),
        ("the landing clause", landing),
        ("the imperative", imperative),
    ] {
        assert_eq!(
            msg.matches(needle).count(),
            1,
            "{who}: {name} must be spelled ONCE, or an index comparison means nothing: {msg}"
        );
    }
    let dq = msg.find(disqualifier).expect("disqualifier missing");
    let land = msg.find(landing).expect("landing missing");
    let verb = msg.find(imperative).expect("imperative missing");
    assert!(
        dq < land,
        "{who}: disqualifier at {dq}, landing at {land}: {msg}"
    );
    assert!(
        land < verb,
        "{who}: the landing is at {land} and the imperative at {verb} -- a reader who acts on the \
         instruction never reaches the caveat behind it: {msg}"
    );
}

/// `schema/stale-updated-at` shipped 115 characters: an observation, no prescription, nowhere to put a
/// qualifier (rule-quality.md §34). Its own disqualifier had to be written along with the landing.
#[test]
fn stale_updated_at_lands_the_orm_ownership_before_the_add_imperative() {
    let msg = schema_issue_message(&issue("stale-updated-at", Some("updatedAt"), None));
    assert_silent_landing_order(
        "stale-updated-at",
        &msg,
        "it will not auto-refresh on writes",
        super::silent_breakage::ORM_TIMESTAMP_OWNERSHIP_LANDING,
        "IF NOTHING ELSE WRITES updatedAt: add",
    );
    for needle in [
        "neither a default nor a constraint",
        "EVERY update of the row",
        "emits no DDL at all",
        "cannot be the same field",
    ] {
        assert!(
            msg.contains(needle),
            "stale-updated-at lost {needle:?}: {msg}"
        );
    }
}

/// `soft-delete-bypass` keeps its EXISTING disqualifier (a `$use`/`$extends` filter this lexical check
/// cannot see) ahead of the landing: whether the finding is true is answered before what acting costs.
#[test]
fn soft_delete_bypass_lands_the_deleted_row_reader_before_the_filter_imperative() {
    let msg = join_issue_message(&join_issue("soft-delete-bypass", Some("deletedAt"), None));
    assert_silent_landing_order(
        "soft-delete-bypass",
        &msg,
        "is invisible to this static check",
        super::silent_breakage::DELETED_ROW_READER_LANDING,
        "IF THIS CALL SITE SHOULD ONLY EVER SEE LIVE ROWS: add",
    );
    for needle in [
        "BECAUSE THIS RULE DID NOT",
        "knows nothing about the function around them",
        "Restore and undelete",
        "which is a legal answer",
    ] {
        assert!(
            msg.contains(needle),
            "soft-delete-bypass lost {needle:?}: {msg}"
        );
    }
}
