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
    // marker for `.sql` files specifically (see `dsl.rs::leaders_for_path`/`compile_marker_sql`) — this is
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

// --- the `#` marker in a `.py` migration: the sentence this message got wrong for a day -------------
//
// `d00b6ad` put `py` into `HASH_COMMENT_EXTENSIONS` and rewrote the five sibling messages that had been
// telling readers no line marker was writable in Python. This rule's message said the same thing and was
// NOT rewritten — it left the bundle before anyone re-read it, and came back still saying it. The pair
// below is what decides that question by behaviour rather than by reading the constant: the control
// proves the rule reaches a `.py` migration at all (without it a suppression test passes on a rule that
// was never going to fire), and the claim proves `#` actually suppresses there.

#[test]
fn an_unmarked_drop_in_a_python_migration_fires() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "alembic/versions/0007_drop_legacy.py",
        "def upgrade():\n    op.execute(\"DROP TABLE legacy_orders\")\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "destructive-migration").len(),
        1,
        "the control for the `#`-marker test below: {:?}",
        out.findings
    );
}

#[test]
fn sql_destructive_migration_hash_ok_marker_in_a_python_migration_suppresses_the_finding() {
    // The message used to say the opposite — that neither `//` nor `--` is writable Python, so the only
    // lever left was a `rules` config entry. `#` has been a marker leader for `.py` since 2026-08-12
    // (`crates/core/src/dsl/markers/path.rs::HASH_COMMENT_EXTENSIONS`), which makes that a whole-file
    // instrument prescribed for a one-line judgment that the one-line instrument already handles.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "alembic/versions/0007_drop_legacy.py",
        "def upgrade():\n    # zzop-destructive-migration-ok: reviewed in PR #482, table fully migrated off\n    op.execute(\"DROP TABLE legacy_orders\")\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "a `#` marker must suppress in a `.py` migration: {:?}",
        out.findings
    );
}

#[test]
fn a_hash_commented_drop_still_fires_in_a_python_migration() {
    // The OTHER half of the same sentence, and the half that is easy to assume away: `#` became a
    // MARKER leader for `.py` in d00b6ad, but the SKIP axis was deliberately left alone
    // (`crates/core/src/dsl/markers/path.rs` — a `#`-commented secret is still a committed secret).
    // This rule sets `skip_comment_lines: true` and admits `.py`, so it is the one place where a reader
    // is most likely to expect `#` to mean "commentary, ignore it". It does not. Same `#`, same file,
    // opposite answers — the message now says both, and this is the assertion under the second half.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "alembic/versions/0008_commented.py",
        "def upgrade():\n    # op.execute(\"DROP TABLE legacy_orders\")\n    pass\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "destructive-migration").len(),
        1,
        "a `#`-commented DROP is read as live code, not skipped: {:?}",
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

// --- the absorbed update-no-where arm: same complete-literal shape as the critical rule -----------
// The arm's pattern is the critical `sql/update-no-where` rule's `line_pattern`, verbatim — the rule
// message's "the same complete-literal shapes ... flag in app code" is a claim these two pins keep
// true. Until 2026-08-03 the arm was a stale copy of the PRE-same-kind-closing shape: any quote kind
// terminated the literal, so a template literal left OPEN on its line (its WHERE riding on the next
// line) read as a complete whole-table write the moment the statement contained an inner quote.

#[test]
fn update_left_open_on_its_line_with_where_on_the_next_line_is_not_flagged() {
    // Never-guess, inherited intact from the critical rule: the backtick literal is NOT closed on
    // this line — the statement continues, and the WHERE lives one line down where a line-scan
    // cannot see it. The old any-quote-terminates copy closed the literal at the inner `"` and
    // disclosed a "whole-table UPDATE" that never existed.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/009_backfill_scoped.ts",
        "export async function up(queryRunner: any) {\n  await queryRunner.query(\n    `UPDATE partner SET \"createId\" = 'legacy'\n     WHERE \"createId\" IS NULL`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn update_closed_literal_with_an_inner_quote_of_another_kind_still_fires() {
    // The boundary the same-kind alignment must NOT cross, pinned from the other direction (the
    // migration copy of the critical rule's `..._inner_quote_of_another_kind_still_fires` pin): an
    // inner `'` inside a `"`-closed literal is statement text, and the whole-table UPDATE around it
    // is still a complete literal this rule must disclose.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/010_anonymize.ts",
        "export async function up(queryRunner: any) {\n  await queryRunner.query(\"UPDATE users SET name = 'anon'\");\n}\n",
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
        Some("update-no-where"),
        "{:?}",
        out.findings
    );
}

#[test]
fn rust_migration_format_interpolated_update_is_silent_in_both_rules() {
    // Unit mirror of the whole-file negative control `cases/trees/rust-svc/migrations/v2_backfill.rs`:
    // a Rust migration interpolating an identifier into an UPDATE template. Out of this rule on the
    // MATCHER, not on the file set: `UPDATE {} SET` gives the `update-no-where` arm no table token,
    // and `require_file` never matches. ⚠ `.rs` IS admitted by `file_pattern` and must stay admitted —
    // `language_scope.rs::destructive_migration_admits_every_extension_its_critical_siblings_exclude`
    // fails if it is dropped, because `sql/delete-no-where` admits `.rs` in app code and excludes
    // migration paths on this rule's behalf. (This comment claimed the opposite until 2026-08-12; a
    // reader who trusted it could have narrowed the extension list and read this green fixture as
    // proof the narrowing was safe.) The critical rule is out by the shared migration-path exclusion —
    // that silence is what routes
    // the line to `security/sql-format-interpolation`'s turf, not to either of these.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/v2_backfill.rs",
        "pub fn backfill_sql(table: &str) -> String {\n    format!(\"UPDATE {} SET migrated = 1\", table)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "{:?}",
        out.findings
    );
    assert!(
        hits(&out, "update-no-where").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- the ORM-METHOD spelling: measured, judged, and deliberately not read -------------------------
//
// `op.drop_table("users")`, `migrationBuilder.DropTable(...)`, `queryRunner.dropTable(...)` and
// `migrations.DeleteModel(...)` are the framework spellings of this rule's own subject, and it does not
// read any of them. For the FIRST THREE that is a JUDGMENT, not something the matcher cannot express:
// widening the drop-or-truncate arm to `DROP[_\s]*(TABLE|COLUMN)` reaches those three, because the arm
// is already case-insensitive and only the separator differs. It does NOT reach Django's
// `migrations.DeleteModel`, which carries neither DROP nor TABLE/COLUMN — that fourth spelling would
// need a token of its own (`DeleteModel`/`RemoveField`), so for it the widening is a reach gap and the
// judgment below does not apply. Recount which spellings the widened arm reaches (ripgrep compiles the
// same `regex` crate the DSL does); 2026-08-14, 4 of the 5 lines match and `migrations.DeleteModel` is
// the one that does not:
//
//   printf '%s\n' 'op.drop_table' 'op.drop_column' 'migrationBuilder.DropTable' \
//       'migrations.DeleteModel' 'queryRunner.dropTable' \
//     | rg -n '(?i)\bDROP[_\s]*(TABLE|COLUMN)\b'
//
// Recount the widened arm's yield on real histories:
//
//   rg --no-ignore -n '(?i)\bDROP[_\s]*(TABLE|COLUMN)\b' \
//      corpus/oss/be-fastapi-fs/backend/app/alembic/versions/ \
//      corpus/oss/be-fastapi/app/db/migrations/versions/
//
// What decided it is what those lines ARE. 2026-08-13, dogfood corpus: 17 such lines, and 14 of them
// sit inside `def downgrade():` — the inverse half Alembic writes for you, which by construction drops
// exactly what the same file's `upgrade()` created. The 3 in an `upgrade()` are one migration replacing
// integer ids with UUIDs. So the widened arm would report the UNDO of a migration as the migration,
// four times for every real one, at a severity whose stated value is "attention on NEW migrations at
// review, not archaeology of old ones". A line-scan cannot separate the two halves: the fact that would
// (the enclosing function's name) is not one it has, and the rule's existing bootstrap veto cannot stand
// in either — it keys on SQL-text `DROP ... IF EXISTS` / `CREATE TABLE` adjacency, which an ORM-method
// file never contains.
//
// The controls that make the 14/17 a measurement rather than an empty set: Django's migration lane IS
// populated in the corpus (7 files, 16 `migrations.*` operations) and spells ZERO destructive ones —
// only `CreateModel`/`AddField`. EF Core and TypeORM/knex are UNMEASURED, not zero: the corpus holds no
// `Migrations/*.cs` and no TS/JS migration directory at all, so this decision rests on the Alembic
// evidence and the Django negative, and a corpus that grows an EF history is the trigger to re-judge.
//
//   rg --no-ignore -o 'migrations\.[A-Za-z]+' corpus/oss/be-django/conduit/apps/*/migrations/*.py \
//     | sed 's/.*://' | sort | uniq -c
//
// The pin below is the disclosure's assertion, and it carries its own anti-vacuity leg: the SQL-text
// control in the SAME admitted path must fire, or the silence would only prove the file set was wrong.
// Ledger: `.claude`-side decision doc is `rules/rule-quality.md` §18.

#[test]
fn the_orm_method_spelling_of_a_destructive_migration_is_not_read() {
    let dir = TempDir::new("zzop-sql");
    // One tree, all four framework spellings, each under a path this rule already admits.
    dir.write(
        "alembic/versions/0009_alembic_orm.py",
        "def upgrade():\n    op.drop_table(\"users\")\n    op.drop_column(\"item\", \"owner_id\")\n",
    );
    dir.write(
        "migrations/0010_django_orm.py",
        "class Migration:\n    operations = [migrations.DeleteModel(name=\"Article\")]\n",
    );
    dir.write(
        "migrations/0011_EfCoreOrm.cs",
        "public partial class Drop : Migration {\n    protected override void Up(MigrationBuilder migrationBuilder) {\n        migrationBuilder.DropTable(name: \"users\");\n    }\n}\n",
    );
    dir.write(
        "migrations/0012-typeorm.ts",
        "export class DropUsers1690000000001 {\n  async up(queryRunner: any) {\n    await queryRunner.dropTable(\"users\");\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "the ORM-method spelling is an intended non-capability, not a silent widening: {:?}",
        out.findings
    );

    // Anti-vacuity: the same directories, the same extensions, the SQL-text spelling — this must fire,
    // or the silence above would be the file set failing rather than the spelling being unread.
    let control = TempDir::new("zzop-sql");
    control.write(
        "alembic/versions/0009_alembic_sql.py",
        "def upgrade():\n    op.execute(\"DROP TABLE users\")\n",
    );
    control.write(
        "migrations/0011_EfCoreSql.cs",
        "public partial class Drop : Migration {\n    protected override void Up(MigrationBuilder mb) {\n        mb.Sql(\"DROP TABLE users\");\n    }\n}\n",
    );
    let control_out = scan(&control);
    assert_eq!(
        hits(&control_out, "destructive-migration").len(),
        2,
        "control: both admitted paths must reach the SQL-text spelling: {:?}",
        control_out.findings
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
