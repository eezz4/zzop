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
        "-- sql-destructive-migration-ok: reviewed in PR #482, table fully migrated off\nDROP TABLE legacy_orders;\n",
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
        "// sql-destructive-migration-ok: reviewed in PR #482, table fully migrated off\nexports.up = (knex) => knex.raw(\"DROP TABLE legacy_orders\");\n",
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
        "-- sql-destructive-migration-ok: reviewed in PR #482, table fully migrated off\nexports.up = (knex) => knex.raw(\"DROP TABLE legacy_orders\");\n",
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
        "-- sql-destructive-migration-ok: reviewed in PR #499\nDROP TABLE stale_sessions;\n",
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
