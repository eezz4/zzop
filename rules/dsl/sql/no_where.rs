use crate::{hits, scan, TempDir};

// --- delete-no-where (critical: complete-literal anchor, never-guess) ---

#[test]
fn delete_from_closed_literal_with_no_where_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function purge(db: any) {\n  return db.query(\"DELETE FROM users\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "delete-no-where");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn delete_from_with_where_clause_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function purge(db: any) {\n  return db.query(\"DELETE FROM users WHERE id = ?\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_from_template_interpolation_is_not_flagged() {
    // `${where}` proves the literal isn't provably closed with no WHERE arriving from elsewhere.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function purge(db: any, where: string) {\n  return db.query(`DELETE FROM users ${where}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_from_string_concatenation_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function purge(db: any, cond: string) {\n  return db.query(\"DELETE FROM users\" + cond);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_from_backtick_concatenation_is_not_flagged() {
    // Review calibration pin: the concat veto must cover the BACKTICK quote class and BOTH concat
    // directions — `` `DELETE FROM users` + cond `` and `cond + "DELETE FROM users"` each carry the
    // WHERE (or its absence) in the concatenated expression, so the closed-literal proof fails and
    // the critical rule must stay silent (never-guess).
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function purge(db: any, cond: string) {\n  return db.query(`DELETE FROM users` + cond);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_from_prefix_concatenation_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function purge(db: any, cond: string) {\n  return db.query(cond + \"DELETE FROM users\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_from_sessions_on_a_line_also_calling_log_somewhere_now_fires() {
    // Regression fixture for the `sql-where-veto` fragment's fix (bare `(?i)WHERE` -> `(?i)\bWHERE\b`):
    // `exclude_pattern` used to veto on ANY substring match of "where", including inside an unrelated
    // identifier elsewhere on the same line — `logSomewhere` contains "where" as a substring
    // (case-insensitively), so a closed, complete-literal `DELETE FROM sessions` with no WHERE clause at
    // all was wrongly suppressed just because `logSomewhere(id)` happened to share the line. With the
    // word-boundary fix, `logSomewhere` no longer matches `\bWHERE\b` (no word boundary before "where" —
    // it's preceded by the letter "e"), so this now correctly fires as the CRITICAL whole-table delete it
    // is.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function purge(db: any, id: string) {\n  db.query(\"DELETE FROM sessions\"); logSomewhere(id);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "delete-no-where");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn delete_from_sessions_with_a_real_where_id_1_clause_is_still_not_flagged() {
    // Paired with the fixture above: a GENUINE `WHERE` clause must still veto — the word-boundary fix
    // narrows the match to real `WHERE` occurrences, it does not stop matching real ones.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function purge(db: any) {\n  return db.query(\"DELETE FROM sessions WHERE id=1\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sql_delete_no_where_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function purge(db: any) {\n  // zzop-delete-no-where-ok: admin-only reset endpoint, reviewed\n  return db.query(\"DELETE FROM users\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_from_no_where_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "tests/db.ts",
        "export async function purge(db: any) {\n  return db.query(\"DELETE FROM users\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_from_no_where_in_a_migration_path_is_destructive_migration_turf_not_critical() {
    // Real-corpus calibration (immich, 564 files): the only delete-no-where hit was a migration
    // backfill (src/schema/migrations/...-AddAssetEditSequence.ts). A whole-table DELETE in a committed
    // migration is a deliberate, reviewed one-time write — critical firing there is severity inflation,
    // so migration paths are excluded from the critical rule and covered by `destructive-migration`
    // (info, disclosure-only) instead.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/schema/migrations/1769105700133-AddAssetEditSequence.ts",
        "export async function up(queryRunner: any) {\n  await queryRunner.query(`DELETE FROM asset_edit_sequence`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
    let h = hits(&out, "destructive-migration");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].severity, zzop_core::Severity::Info);
    assert_eq!(
        h[0].data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|l| l.as_str()),
        Some("delete-no-where"),
        "{:?}",
        out.findings
    );
}

// --- update-no-where (critical: complete-literal anchor, never-guess) ---

#[test]
fn update_set_closed_literal_with_no_where_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function activateAll(db: any) {\n  return db.query(\"UPDATE users SET active = 1\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "update-no-where");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn update_set_with_where_clause_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function activate(db: any) {\n  return db.query(\"UPDATE users SET active = 1 WHERE id = ?\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn update_set_template_interpolation_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function activate(db: any, where: string) {\n  return db.query(`UPDATE users SET active = 1 ${where}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn update_set_string_concatenation_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function activateAll(db: any, cond: string) {\n  return db.query(\"UPDATE users SET active = 1\" + cond);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- update-no-where, Rust `format!` interpolation (the `.rs` lane's false-positive class) ---
//
// The arc that added `.rs` to this rule's `file_pattern` left the interpolation proof JS-only: the
// `${...}`/concat vetoes in `${sql-where-veto}` know nothing about Rust's `{}` / `{name}` format
// placeholders, so `format!("UPDATE accounts SET balance = {}", b)` matched the "closed literal, no
// WHERE" shape and fired at CRITICAL — telling the reader a whole-table update was proven when the
// value (and any WHERE riding with it) is spliced in at runtime. The fix is in `line_pattern`, not in
// the shared veto: the evidence lives INSIDE the literal (`[^"'`{}]*` between `SET` and the closing
// quote), which is the only field that can see inside it, and that keeps the three rules sharing
// `${sql-where-veto}` — and every `.ts` finding — byte-identically where they were.

#[test]
fn update_set_rust_format_positional_placeholder_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/db.rs",
        "pub fn zero_balances(b: i64) -> String {\n    format!(\"UPDATE accounts SET balance = {}\", b)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn update_set_rust_format_named_placeholder_is_not_flagged() {
    // Rust's inline-named form takes no trailing argument at all, so "the macro has no args" is not a
    // usable signal either — the placeholder itself is the whole proof.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/db.rs",
        "pub fn zero_balances(bal: i64) -> String {\n    let _ = bal;\n    format!(\"UPDATE accounts SET balance = {bal}\")\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn update_set_rust_format_with_an_interpolated_where_clause_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/db.rs",
        "pub fn zero_one(b: i64) -> String {\n    format!(\"UPDATE accounts SET balance = {} WHERE id = 1\", b)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The under-detection boundary the fix must NOT cross: a Rust literal with no placeholder in it is
/// still a complete statement, and a whole-table UPDATE written that way is exactly what this rule is
/// for. Dropping `.rs` from the `file_pattern` would have silenced this too.
#[test]
fn update_set_rust_closed_literal_with_no_placeholder_still_fires() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/db.rs",
        "pub fn zero_balances(conn: &Conn) -> usize {\n    conn.execute(\"UPDATE accounts SET balance = 0\")\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "update-no-where");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

/// Second half of that boundary: the `{}`/`{name}` evidence only counts INSIDE the SQL literal. A brace
/// pair elsewhere on the line (here an empty struct literal passed as the params argument) is not
/// interpolation into the statement, so it must not launder a genuine whole-table UPDATE — the
/// line-wide veto this fix deliberately did not widen is what would have gotten this wrong.
#[test]
fn update_set_rust_closed_literal_with_an_unrelated_brace_pair_on_the_line_still_fires() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/db.rs",
        "pub fn zero_balances(conn: &Conn) -> usize {\n    conn.execute(\"UPDATE accounts SET balance = 0\", Params {})\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "update-no-where");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

/// The partition promise, honored: the message says a `{}` between the `SET` and the closing quote
/// keeps this rule silent, but the old matcher accepted ANY quote kind as the terminator — the inner
/// `'` in `format!("UPDATE users SET name = '{}'", name)` was read as the literal's closing quote,
/// the `{}` fell outside the scanned span, and this rule co-fired at CRITICAL on top of
/// `security/sql-format-interpolation`, telling the reader two different stories about one line.
/// Same-kind quote termination (each alternation branch closes only with the quote that opened it)
/// makes the message's sentence true: this line is the warning-severity interpolation rule's alone.
#[test]
fn update_set_rust_format_with_the_placeholder_inside_inner_single_quotes_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/db.rs",
        "pub fn rename_all(name: &str) -> String {\n    format!(\"UPDATE users SET name = '{}'\", name)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The boundary the same-kind fix must NOT cross, pinned from the other direction: an inner quote of
/// a DIFFERENT kind with NO placeholder is statement text inside a genuinely closed literal — a
/// whole-table UPDATE that must keep firing. A fix that treated any inner quote as "can't prove
/// closed" would have silently traded the false co-fire for this false negative.
#[test]
fn update_set_closed_literal_with_an_inner_quote_of_another_kind_still_fires() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/db.rs",
        "pub fn anonymize(conn: &Conn) -> usize {\n    conn.execute(\"UPDATE users SET name = 'anon'\")\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "update-no-where");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

/// The other half of the `.rs` lane: the concat veto in `${sql-where-veto}` was spelled for JS
/// (`"literal" +`), and Rust cannot write that — `&str` has no `Add`, so the compiling form is
/// `"literal".to_string() + expr`, which puts `.to_string()` between the quote and the `+` and sailed
/// past the veto. A WHERE riding in `cond` would have been invisible and the rule would have fired at
/// CRITICAL. `.to_string()`/`.to_owned()` is Rust-only vocabulary (JS spells it `.toString()`, a
/// different token), so admitting it into the shared fragment cannot move a `.ts` finding.
#[test]
fn update_set_rust_to_string_concatenation_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/db.rs",
        "pub fn zero_some(cond: &str) -> String {\n    \"UPDATE accounts SET balance = 0\".to_string() + cond\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sql_update_no_where_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function activateAll(db: any) {\n  // zzop-update-no-where-ok: admin-only bulk reactivation, reviewed\n  return db.query(\"UPDATE users SET active = 1\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn update_set_no_where_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "tests/db.ts",
        "export async function activateAll(db: any) {\n  return db.query(\"UPDATE users SET active = 1\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn update_set_no_where_in_a_migration_path_is_destructive_migration_turf_not_critical() {
    // Same calibration as the DELETE sibling above (immich hit:
    // src/schema/migrations/...-PartnerCreateId.ts) — a whole-table UPDATE backfill in a committed
    // migration is deliberate, so it routes to `destructive-migration` at info, not critical.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/schema/migrations/1750107668827-PartnerCreateId.ts",
        "export async function up(queryRunner: any) {\n  await queryRunner.query(`UPDATE partner SET \"createId\" = \"updateId\"`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
    let h = hits(&out, "destructive-migration");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].severity, zzop_core::Severity::Info);
    assert_eq!(
        h[0].data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|l| l.as_str()),
        Some("update-no-where"),
        "{:?}",
        out.findings
    );
}
