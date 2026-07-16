use crate::{hits, scan, TempDir};

// --- sql-delete-no-where (critical: complete-literal anchor, never-guess) ---

#[test]
fn delete_from_closed_literal_with_no_where_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function purge(db: any) {\n  return db.query(\"DELETE FROM users\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-delete-no-where");
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
        hits(&out, "sql-delete-no-where").is_empty(),
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
        hits(&out, "sql-delete-no-where").is_empty(),
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
        hits(&out, "sql-delete-no-where").is_empty(),
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
        hits(&out, "sql-delete-no-where").is_empty(),
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
        hits(&out, "sql-delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sql_delete_no_where_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function purge(db: any) {\n  // sql-delete-no-where-ok: admin-only reset endpoint, reviewed\n  return db.query(\"DELETE FROM users\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-delete-no-where").is_empty(),
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
        hits(&out, "sql-delete-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn delete_from_no_where_in_a_migration_path_is_destructive_migration_turf_not_critical() {
    // Real-corpus calibration (immich, 564 files): the only sql-delete-no-where hit was a migration
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
        hits(&out, "sql-delete-no-where").is_empty(),
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

// --- sql-update-no-where (critical: complete-literal anchor, never-guess) ---

#[test]
fn update_set_closed_literal_with_no_where_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function activateAll(db: any) {\n  return db.query(\"UPDATE users SET active = 1\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-update-no-where");
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
        hits(&out, "sql-update-no-where").is_empty(),
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
        hits(&out, "sql-update-no-where").is_empty(),
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
        hits(&out, "sql-update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sql_update_no_where_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "db.ts",
        "export async function activateAll(db: any) {\n  // sql-update-no-where-ok: admin-only bulk reactivation, reviewed\n  return db.query(\"UPDATE users SET active = 1\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-update-no-where").is_empty(),
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
        hits(&out, "sql-update-no-where").is_empty(),
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
        hits(&out, "sql-update-no-where").is_empty(),
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
