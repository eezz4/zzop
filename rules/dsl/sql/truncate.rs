use crate::{hits, scan, TempDir};

// --- truncate-in-app-code / destructive-migration (same TRUNCATE line routed by path) ---

#[test]
fn truncate_in_app_code_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/cleanup.ts",
        "export async function reset(db: any) {\n  return db.exec(`TRUNCATE TABLE users`);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "truncate-in-app-code");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert!(
        hits(&out, "destructive-migration").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn truncate_in_a_migration_sql_file_fires_destructive_migration_not_truncate_in_app_code() {
    let dir = TempDir::new("zzop-sql");
    dir.write("migrations/001_init.sql", "TRUNCATE TABLE users;\n");
    let out = scan(&dir);
    let h = hits(&out, "destructive-migration");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert!(
        hits(&out, "truncate-in-app-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn truncate_in_a_ts_migration_file_is_excluded_from_truncate_in_app_code() {
    // Same quoted-literal shape as the app-code positive, but under migrations/ — the file_exclude_pattern's
    // migration-path alternative (not just an extension mismatch) is what silences this one.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "migrations/001_init.ts",
        "export async function up(db: any) {\n  return db.exec(`TRUNCATE TABLE users`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "truncate-in-app-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn truncate_in_app_code_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "tests/cleanup.ts",
        "export async function reset(db: any) {\n  return db.exec(`TRUNCATE TABLE users`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "truncate-in-app-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sql_truncate_app_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/cleanup.ts",
        "export async function reset(db: any) {\n  // zzop-truncate-in-app-code-ok: dedicated nightly cache-reset job\n  return db.exec(`TRUNCATE TABLE users`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "truncate-in-app-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn jsx_truncate_boolean_prop_is_not_flagged() {
    // A JSX boolean prop `truncate` sits after the CLOSING quote of
    // a sibling attribute (`size="sm" truncate style=...`). The rule now requires a CLOSED string
    // literal (a quote after the table name, like its `delete-no-where` siblings), so `truncate`
    // as prose outside any quoted SQL string no longer fires.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/ui/Row.tsx",
        "export const Row = () => <MonoText size=\"sm\" truncate style={{ flex: 1 }}>hi</MonoText>;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "truncate-in-app-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn tailwind_truncate_class_name_is_not_flagged() {
    // Same class fix, broader surface than the review noted: Tailwind's `truncate` text-overflow
    // utility opens the className string (`"truncate w-full"`), so the quote IS adjacent to
    // TRUNCATE — only the closed-literal requirement (no closing quote right after a table name)
    // keeps this from firing across every React frontend.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/ui/Card.tsx",
        "export const Card = () => <div className=\"truncate w-full text-sm\">x</div>;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "truncate-in-app-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn truncate_bare_table_without_the_table_keyword_is_still_flagged() {
    // The closed-literal tightening must not lose the bare `TRUNCATE <table>` form (valid on
    // Postgres/MySQL) — the `(TABLE\s+)?` group stays optional.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/cleanup.ts",
        "export async function reset(db: any) {\n  return db.exec(`TRUNCATE sessions`);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "truncate-in-app-code").len(),
        1,
        "{:?}",
        out.findings
    );
}

// --- the Python comment leader: two tables, one file type ------------------------------------------
//
// This rule's message says two things about a `.py` file that come from DIFFERENT engine tables, and it
// said them with ONE word ("the comment/marker leader") until 2026-08-12 — true of at most one of the
// two, and self-contradicted four clauses later by the `#` marker it offers. `scripts/check-marker-claims.sh`
// now refuses the fused wording; the three fixtures below decide the same question by BEHAVIOUR, so a
// future edit cannot satisfy the guard with prose the engine does not back. The control is not optional:
// without it the suppression assertion passes just as happily on a rule that was never going to fire in
// a `.py` file at all.
const PY_TRUNCATE: &str = "def reset(db):\n    db.exec(\"TRUNCATE TABLE users\")\n";

#[test]
fn an_unmarked_truncate_in_python_app_code_fires() {
    let dir = TempDir::new("zzop-sql");
    dir.write("src/cleanup.py", PY_TRUNCATE);
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "truncate-in-app-code").len(),
        1,
        "the control for the two `#` tests below: {:?}",
        out.findings
    );
}

#[test]
fn sql_truncate_app_hash_ok_marker_in_python_suppresses_the_finding() {
    // The MARKER axis: `py` is in `HASH_COMMENT_EXTENSIONS`, so `# <marker>` suppresses exactly as
    // `// <marker>` does elsewhere — which is what makes the message's closing offer writable in a
    // language that cannot spell `//` at all.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/cleanup.py",
        "def reset(db):\n    # zzop-truncate-in-app-code-ok: dedicated nightly cache-reset job\n    db.exec(\"TRUNCATE TABLE users\")\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "truncate-in-app-code").is_empty(),
        "a `#` marker must suppress in a `.py` file: {:?}",
        out.findings
    );
}

#[test]
fn a_hash_commented_truncate_still_fires_in_python() {
    // The SKIP axis, deliberately NOT widened alongside the marker axis: `skip_comment_lines` still
    // reads `//` alone outside `.sql` and config files, so a `#`-commented-out TRUNCATE is judged as
    // live code. This is the half a reader is most likely to get backwards here — the same `#` that
    // silences a finding one line above does NOT make the line below it invisible.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/cleanup.py",
        "def reset(db):\n    # db.exec(\"TRUNCATE TABLE users\")\n    pass\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "truncate-in-app-code").len(),
        1,
        "a `#`-commented-out TRUNCATE is still live code to the skip axis: {:?}",
        out.findings
    );
}
