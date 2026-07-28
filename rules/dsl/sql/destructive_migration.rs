use crate::{hits, scan, TempDir};

// --- destructive-migration (info: migration paths only) ---
// Severity calibration (real corpus): immich's migration history alone produced 93 deliberate DROP hits —
// at warning that floods the baseline and breaks a failOn:warn gate on a healthy repo. Info is
// disclosure-only: this rule's value is review-time attention on NEW migrations, not archaeology of old
// ones. It also absorbs the closed-literal whole-table DELETE/UPDATE shapes the critical rules exclude
// from migration paths (see the two `..._is_destructive_migration_turf_not_critical` fixtures above).

#[test]
fn drop_table_in_a_migration_file_is_flagged_at_info() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/002_drop_legacy.sql",
        "DROP TABLE legacy_orders;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "destructive-migration");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].severity, zzop_core::Severity::Info);
    assert_eq!(
        h[0].data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|l| l.as_str()),
        Some("drop-or-truncate"),
        "{:?}",
        out.findings
    );
}

#[test]
fn drop_column_in_a_typeorm_migration_ts_file_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/1690000000000-DropLegacyColumn.ts",
        "export class DropLegacyColumn1690000000000 {\n  async up(queryRunner: any) {\n    await queryRunner.query(\"DROP TABLE legacy_column\");\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "destructive-migration").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn drop_table_outside_a_migration_path_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/schema.ts",
        "// raw admin script, not a migration\nconst sql = \"DROP TABLE legacy_orders\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sql_destructive_migration_dash_dash_ok_marker_in_a_sql_migration_file_suppresses_the_finding() {
    // `.sql` files use `--` line comments, not `//`, so the marker recognizer accepts a `--`-comment
    // marker for `.sql` files specifically (see `dsl.rs::is_sql_file`/`compile_marker_sql`) — this is
    // what lets a migration DROP be suppressed inline instead of only tree-wide via `disabled_rules`.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/003_drop_reviewed.sql",
        "-- zzop-destructive-migration-ok: reviewed in PR #482, table fully migrated off\nDROP TABLE legacy_orders;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sql_destructive_migration_ok_marker_in_a_js_migration_file_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/004_drop_reviewed.js",
        "// zzop-destructive-migration-ok: reviewed in PR #482, table fully migrated off\nexports.up = (knex) => knex.raw(\"DROP TABLE legacy_orders\");\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sql_destructive_migration_dash_dash_marker_text_in_a_js_migration_file_does_not_suppress() {
    // The `--`-comment recognizer is gated to `.sql` files only: `--` is not a comment in JS/TS (`--x` is
    // a decrement there), so the same marker text in a `.js` migration file must NOT suppress the finding
    // — only the `//` form (covered above) works there.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/005_drop_reviewed.js",
        "-- zzop-destructive-migration-ok: reviewed in PR #482, table fully migrated off\nexports.up = (knex) => knex.raw(\"DROP TABLE legacy_orders\");\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "destructive-migration").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn sql_destructive_migration_dash_dash_marker_above_the_drop_line_in_sql_also_suppresses() {
    // Same 1-line lookback window as the `//` form: the marker on the line directly above the DROP still
    // suppresses it, not just a marker on the DROP line itself.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/006_drop_reviewed.sql",
        "-- zzop-destructive-migration-ok: reviewed in PR #499\nDROP TABLE stale_sessions;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sql_destructive_migration_unmarked_drop_in_sql_still_fires() {
    // Baseline: a `.sql` migration DROP with no marker at all must still fire at info — the `--`-marker
    // gate only suppresses when the marker text is actually present, never silences `.sql` files broadly.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/007_drop_unreviewed.sql",
        "DROP TABLE unreviewed_orders;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "destructive-migration");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].severity, zzop_core::Severity::Info);
}

#[test]
fn sql_destructive_migration_unrelated_dash_dash_marker_text_does_not_suppress() {
    // A `--`-comment that names a DIFFERENT marker must not suppress — mirrors
    // `unrelated_marker_text_does_not_suppress` in `crates/core/src/dsl.rs` for the `//` form.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/008_drop_unrelated.sql",
        "-- some-other-marker-ok: not this rule\nDROP TABLE other_orders;\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "destructive-migration").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn bootstrap_drop_if_exists_followed_by_create_table_is_not_flagged() {
    // Calibration pin (7/7 corpus FPs before the fix): an idempotent bootstrap preamble in the very
    // first migration — every `DROP TABLE IF EXISTS x;` is immediately followed by `CREATE TABLE x`
    // in the SAME file, so re-running the file destroys nothing that the file does not recreate.
    // The veto keys on that in-file DROP-then-CREATE evidence, NOT on the `0001_` filename ordinal.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/0001_initial.sql",
        "DROP TABLE IF EXISTS users;\nCREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT);\nDROP TABLE IF EXISTS sessions;\nCREATE TABLE sessions (id INTEGER PRIMARY KEY, user_id INTEGER);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn drop_if_exists_with_no_create_in_the_file_still_fires() {
    // Positive pin for the same narrowing: `IF EXISTS` on its own is NOT the exemption — a defensive
    // drop in a later migration with nothing recreating anything is exactly the destructive change
    // this rule discloses at review.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/0042_drop_legacy.sql",
        "DROP TABLE IF EXISTS legacy_orders;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "destructive-migration");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].severity, zzop_core::Severity::Info);
}

#[test]
fn a_dash_dash_comment_between_the_drop_and_the_create_still_counts_as_bootstrap() {
    // Scope pin for the message's "nothing between them but whitespace and `--` comments" wording —
    // real bootstrap files annotate each table, so "immediately followed by" would have been a lie.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/0001_annotated.sql",
        "DROP TABLE IF EXISTS users;\n-- users: one row per account\nCREATE TABLE users (id INTEGER PRIMARY KEY);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_plain_drop_table_above_a_create_table_is_not_bootstrap_and_still_fires() {
    // Scope pin for the message's "`IF EXISTS` is required" claim: without it the statement is not a
    // re-runnable bootstrap, so a rename-style drop-then-create is still disclosed.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/0003_rename.sql",
        "DROP TABLE customer_invoices;\nCREATE TABLE invoices (id INTEGER PRIMARY KEY);\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "destructive-migration").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn a_drop_that_shares_a_file_with_a_bootstrap_preamble_is_the_documented_residual() {
    // Honest residual pin: the veto is whole-FILE (line-scan cannot correlate the DROP's table name
    // with the CREATE's — the regex engine has no backreferences), so a genuinely destructive drop
    // sharing a file with a bootstrap preamble is missed. Pinned so the limitation is a KNOWN,
    // reviewed cost rather than a surprise, and so a future name-correlating matcher flips it.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/0002_mixed.sql",
        "DROP TABLE IF EXISTS cache_entries;\nCREATE TABLE cache_entries (id INTEGER PRIMARY KEY);\nDROP TABLE customer_invoices;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn where_scoped_delete_in_a_migration_is_not_flagged() {
    // The absorbed DELETE/UPDATE alternatives carry the same never-guess discipline as the critical
    // rules: a WHERE-scoped statement is a filtered subset, not a whole-table write, and stays silent.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/005_cleanup.ts",
        "export async function up(queryRunner: any) {\n  await queryRunner.query(`DELETE FROM sessions WHERE expired = true`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "{:?}",
        out.findings
    );
}
