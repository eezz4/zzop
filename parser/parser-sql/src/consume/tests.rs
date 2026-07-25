use super::extract_statement_table_refs;

// --- corpus-derived shapes -------------------------------------------------------------------
// The five statements below are copied byte-for-byte out of the Cloudflare-D1 dogfood corpus
// (`settle-hub-be/src/createLedger.ts`, `ping-hub-be/src/createGroup.ts`,
// `schedule-hub-be/src/createSchedule.ts`) — the anchors this extractor was built for.

#[test]
fn corpus_select_with_order_by_and_limit() {
    assert_eq!(
        extract_statement_table_refs(
            "SELECT revision_no FROM ledger_revision WHERE ledger_id = ? ORDER BY revision_no DESC LIMIT 1"
        ),
        vec!["ledger_revision"]
    );
}

#[test]
fn corpus_insert_into() {
    assert_eq!(
        extract_statement_table_refs(
            "INSERT INTO ledger_revision (id, ledger_id, revision_no, snapshot, created_at) VALUES (?, ?, ?, ?, ?)"
        ),
        vec!["ledger_revision"]
    );
}

#[test]
fn corpus_update_set() {
    assert_eq!(
        extract_statement_table_refs("UPDATE ledger SET updated_at = ? WHERE id = ?"),
        vec!["ledger"]
    );
}

#[test]
fn corpus_count_star_select() {
    assert_eq!(
        extract_statement_table_refs("SELECT COUNT(*) as cnt FROM sessions WHERE group_id = ?"),
        vec!["sessions"]
    );
}

#[test]
fn corpus_insert_or_replace_into() {
    assert_eq!(
        extract_statement_table_refs(
            "INSERT OR REPLACE INTO responses (schedule_id, participant_name, available_slots, proposed, updated_at) VALUES (?, ?, ?, ?, ?)"
        ),
        vec!["responses"]
    );
}

#[test]
fn corpus_delete_from() {
    assert_eq!(
        extract_statement_table_refs("DELETE FROM sessions WHERE group_id = ? AND nickname = ?"),
        vec!["sessions"]
    );
}

// --- statement-shape gate (synthetic, NOT corpus-derived) ------------------------------------

#[test]
fn prose_beginning_with_a_sql_keyword_is_not_a_statement() {
    // The gate's whole reason to exist: an i18n label must never mint a `db-table` key. The first two
    // are structurally valid SQL (`SELECT <col> FROM <table> <alias>`) and are stopped by case alone.
    assert!(extract_statement_table_refs("Select a date from the calendar").is_empty());
    assert!(extract_statement_table_refs("select the invoice from history").is_empty());
    assert!(extract_statement_table_refs("Update your profile").is_empty());
    assert!(extract_statement_table_refs("Delete this item?").is_empty());
}

#[test]
fn a_sql_mention_mid_string_is_not_a_statement() {
    assert!(extract_statement_table_refs("run this: SELECT id FROM users").is_empty());
}

#[test]
fn leading_whitespace_and_newlines_still_open_a_statement() {
    assert_eq!(
        extract_statement_table_refs("\n      SELECT id\n      FROM users\n      WHERE id = ?\n"),
        vec!["users"]
    );
}

#[test]
fn lowercase_keywords_are_an_honest_under_approximation() {
    // Case sensitivity IS the precision gate (module doc): it is the only thing separating a query
    // from prose like "Select a date from the list", which is a structurally valid SELECT.
    assert!(extract_statement_table_refs("select id from users where id = ?").is_empty());
    assert!(extract_statement_table_refs("Select a date from the list").is_empty());
}

#[test]
fn a_table_name_keeps_its_own_case_through_the_channel_transform() {
    assert_eq!(
        extract_statement_table_refs("SELECT id FROM Users WHERE id = ?"),
        vec!["users"],
        "channel casing lower-firsts, same as the DDL provide side"
    );
}

// --- multi-table / qualified / quoted (synthetic, NOT corpus-derived) ------------------------

#[test]
fn join_contributes_both_sides_in_first_appearance_order() {
    assert_eq!(
        extract_statement_table_refs(
            "SELECT s.id FROM sessions s INNER JOIN groups g ON g.id = s.group_id"
        ),
        vec!["sessions", "groups"]
    );
}

#[test]
fn a_repeated_table_is_reported_once() {
    assert_eq!(
        extract_statement_table_refs("SELECT a.id FROM users a JOIN users b ON b.id = a.parent"),
        vec!["users"]
    );
}

#[test]
fn schema_qualifier_is_dropped_and_quotes_stripped_like_the_ddl_side() {
    assert_eq!(
        extract_statement_table_refs("SELECT id FROM \"public\".\"Article\" WHERE id = ?"),
        vec!["article"]
    );
    assert_eq!(
        extract_statement_table_refs("SELECT id FROM [dbo].[Orders]"),
        vec!["orders"]
    );
}

#[test]
fn a_subquery_source_contributes_only_its_inner_table() {
    assert_eq!(
        extract_statement_table_refs("SELECT x FROM (SELECT id AS x FROM users) t"),
        vec!["users"]
    );
}

// --- never-guess vetoes (synthetic, NOT corpus-derived) --------------------------------------

#[test]
fn extract_from_names_a_column_not_a_table() {
    assert_eq!(
        extract_statement_table_refs("SELECT EXTRACT(YEAR FROM created_at) FROM orders"),
        vec!["orders"],
        "the function-argument FROM must not mint table:created_at"
    );
}

#[test]
fn trim_and_substring_from_are_vetoed_too() {
    assert_eq!(
        extract_statement_table_refs("SELECT SUBSTRING(name FROM 1 FOR 3) FROM users"),
        vec!["users"]
    );
    assert_eq!(
        extract_statement_table_refs("SELECT TRIM(BOTH ' ' FROM label) FROM tags"),
        vec!["tags"]
    );
}

#[test]
fn a_cte_name_is_not_a_table() {
    assert_eq!(
        extract_statement_table_refs(
            "WITH recent AS (SELECT id FROM orders LIMIT 10) SELECT * FROM recent"
        ),
        vec!["orders"],
        "recent is a query-local alias — minting table:recent would false-join a real table"
    );
}

#[test]
fn a_second_cte_after_a_comma_is_vetoed_as_well() {
    assert_eq!(
        extract_statement_table_refs(
            "WITH a AS (SELECT id FROM orders), b AS (SELECT id FROM users) SELECT * FROM a JOIN b ON a.id = b.id"
        ),
        vec!["orders", "users"]
    );
}

// --- nothing extractable (synthetic, NOT corpus-derived) -------------------------------------

#[test]
fn empty_and_non_sql_strings_yield_nothing() {
    assert!(extract_statement_table_refs("").is_empty());
    assert!(extract_statement_table_refs("/api/users").is_empty());
}

#[test]
fn ddl_is_not_a_consume() {
    // `CREATE TABLE` is the PROVIDE side's business (`crate::extract`); this function must stay silent
    // so a schema string embedded in application code never reads as a table access.
    assert!(extract_statement_table_refs("CREATE TABLE users (id TEXT PRIMARY KEY)").is_empty());
}
