//! The Rust lane of this pack: the two Rust-native rules (`sql-format-interpolation`,
//! `command-and-interpolation`) and the `rust-str-const` arm `hardcoded-secret` grew for Rust's typed
//! `const NAME: &str = "…"` form.
//!
//! Why the arm exists at all, pinned here rather than only stated in the message: the pre-existing
//! `assignment` arm requires the VALUE to follow the NAME directly (`name = "literal"`), and Rust's
//! idiomatic constant puts a type in between. Widening `assignment` itself was rejected — it would have
//! had to admit an arbitrary token run between the name and the `=`, which is exactly how a
//! `key: string = getFromVault()` shape in another language starts matching.

use crate::{hits, label_of, scan, TempDir};

// --- sql-format-interpolation ---

#[test]
fn a_format_macro_building_a_select_statement_is_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/queries.rs",
        "pub fn by_name(name: &str) -> String {\n    format!(\"SELECT id, email FROM users WHERE name = '{}'\", name)\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-format-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn a_write_macro_building_an_update_statement_is_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/queries.rs",
        "use std::fmt::Write;\npub fn bump(buf: &mut String, id: i64) {\n    let _ = write!(buf, \"UPDATE accounts SET hits = hits + 1 WHERE id = {}\", id);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-format-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

/// Pins the message's claim "A SQL literal with no placeholder is not matched (nothing is being spliced
/// in)". The statement is still assembled by `format!`, so only the missing `{` keeps it quiet.
#[test]
fn a_format_macro_whose_sql_literal_carries_no_placeholder_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/queries.rs",
        "pub fn all() -> String {\n    format!(\"SELECT id, email FROM users\")\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-format-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

/// Pins the other half of the same sentence — "neither is a `format!` whose template carries no SQL
/// statement keyword". The file still has to satisfy `require_file`, so the SQL vocabulary is present in
/// the file and absent from the matched line, which is the case a whole-file pre-gate cannot decide.
#[test]
fn a_format_macro_with_a_placeholder_but_no_sql_keyword_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/queries.rs",
        "pub const NOTE: &str = \"the SELECT below is parameterized\";\npub fn label(id: i64) -> String {\n    format!(\"user #{}\", id)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-format-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_parameterized_statement_kept_constant_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/queries.rs",
        "pub fn by_name() -> &'static str {\n    \"SELECT id, email FROM users WHERE name = $1\"\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-format-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The 2026-08-03 widening's whole point: a placeholder BEFORE the statement's second keyword — the
/// dynamic column list, the classic non-parameterizable injection vector — now fires. The previous
/// matcher only looked for `{` AFTER the keyword pair, so this exact shape was silent.
#[test]
fn a_placeholder_before_the_from_keyword_dynamic_columns_is_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/queries.rs",
        "pub fn projection(cols: &str) -> String {\n    format!(\"SELECT {} FROM users\", cols)\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-format-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

/// A placeholder in the TABLE position of an UPDATE — the other before-the-second-keyword direction.
#[test]
fn a_placeholder_in_the_update_table_position_is_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/queries.rs",
        "pub fn touch(table: &str) -> String {\n    format!(\"UPDATE {} SET touched_at = now()\", table)\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-format-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

/// The other half of the same 2026-08-03 change: the template must BEGIN with the statement. A log line
/// that mentions SQL keywords mid-sentence builds no statement text and must stay silent — before the
/// begins-with anchor this exact line fired.
#[test]
fn a_log_string_mentioning_sql_keywords_mid_sentence_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/cache.rs",
        "pub fn cache_delete_error(key: &str) -> String {\n    format!(\"failed to DELETE FROM cache for key {}\", key)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-format-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The uppercase-only statement gate, prose direction 1: sentence-case English that is ALSO a
/// syntactically valid `SELECT <col> FROM <table> ...` head. The begins-with anchor alone cannot
/// exclude it — `"Select {} from the list ..."` BEGINS with its "statement" — so with the
/// case-insensitive matcher this rule shipped with, this exact line fired (measured on a real
/// binary). Nothing but case separates SQL from prose here, the same call
/// `sql/raw-sql-check-then-write` makes for the same reason. The `NOTE` const keeps `require_file`
/// (now uppercase-only too) satisfied, so it is the LINE gate this fixture pins, not the file gate.
/// The uppercase positives above (`a_format_macro_building_a_select_statement_is_flagged` and
/// friends) are this test's other half: the gate removes the prose, not the statements.
#[test]
fn an_english_sentence_starting_with_select_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/regions.rs",
        "pub const NOTE: &str = \"the SELECT queries in this module are parameterized\";\npub fn prompt(choice: &str) -> String {\n    format!(\"Select {} from the list of available regions\", choice)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-format-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

/// Prose direction 2, the `UPDATE ... SET` arm: `"Update {} set to {}"` reads as
/// `UPDATE <table> SET ...` to a case-insensitive matcher (also measured firing). Same gate, same
/// pairing with `a_write_macro_building_an_update_statement_is_flagged` above.
#[test]
fn an_english_sentence_starting_with_update_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/settings.rs",
        "pub const NOTE: &str = \"persisted via UPDATE with bound parameters\";\npub fn confirm(field: &str, value: &str) -> String {\n    format!(\"Update {} set to {}\", field, value)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-format-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The gate's disclosed cost, pinned so it stays DISCLOSED rather than drifting into "bug nobody
/// wrote down": lowercase SQL is a real statement and this rule cannot see it — the message says so.
/// The uppercase `SELECT` const keeps the file gate satisfied; the silence below is the line gate's.
#[test]
fn a_lowercase_sql_statement_is_the_uppercase_gates_disclosed_under_report() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/queries.rs",
        "pub const BASE: &str = \"SELECT id FROM users WHERE name = $1\";\npub fn by_name(name: &str) -> String {\n    format!(\"select id, email from users where name = '{}'\", name)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-format-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The fragment-alignment half of the same batch: this rule now shares `${test-paths-migrations}` with
/// its `sql/` no-where siblings, so the SAME interpolating statement inside a migrations directory —
/// the canonical legitimate dynamic SQL, with nothing request-derived in reach — is excluded, exactly
/// as those siblings already excluded their placeholder-free twins there.
#[test]
fn an_interpolating_statement_in_a_migrations_path_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "migrations/v7_backfill.rs",
        "pub fn backfill(table: &str) -> String {\n    format!(\"UPDATE {} SET migrated = 1\", table)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-format-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn the_sql_format_interpolation_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/queries.rs",
        "pub fn by_name(name: &str) -> String {\n    // zzop-sql-format-interpolation-ok: `name` is an internal enum discriminant, never request-derived\n    format!(\"SELECT id, email FROM users WHERE name = '{}'\", name)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-format-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- command-and-interpolation ---

#[test]
fn command_new_and_a_format_in_one_function_body_is_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/jobs.rs",
        "use std::process::Command;\npub fn run(period: &str) -> std::io::Result<std::process::Output> {\n    let script = format!(\"/usr/local/bin/report --period {}\", period);\n    Command::new(\"sh\").arg(\"-c\").arg(script).output()\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "command-and-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    // The trigger is the INTERPOLATION, not the spawn — so the finding lands on the line that built the
    // string, which is the line a reader has to judge.
    assert_eq!(h[0].line, 3);
}

#[test]
fn command_new_with_separate_args_and_no_interpolation_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/jobs.rs",
        "use std::process::Command;\npub fn run(period: &str) -> std::io::Result<std::process::Output> {\n    Command::new(\"/usr/local/bin/report\").arg(\"--period\").arg(period).output()\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "command-and-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The `-and-` in the id claims co-occurrence in ONE function, and this is what makes that claim true
/// rather than file-wide: `require_file` is satisfied and both patterns exist in the file, but they sit
/// in different symbol body spans.
#[test]
fn an_interpolation_in_a_different_function_from_the_spawn_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/jobs.rs",
        "use std::process::Command;\npub fn label(period: &str) -> String {\n    format!(\"report for {}\", period)\n}\npub fn run() -> std::io::Result<std::process::Output> {\n    Command::new(\"/usr/local/bin/report\").output()\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "command-and-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

/// Regression pin for the retired `"[^"]*"\s*\+\s*&?[A-Za-z_]` arm. That arm was written for a
/// `"literal" + expr` string concatenation, which does not exist in Rust — `&str` has no `Add`, so
/// `"echo " + &name` does not compile and `"echo ".to_string() + &name` / `String::from("echo ") + &name`
/// (the forms that DO compile) put `.to_string()` / `)` between the quote and the `+`, so the arm never
/// matched one of them. What it DID match is this: a literal containing ESCAPED quotes, where the
/// regex's idea of where the string ends is an artifact of the escaping (`"a\"` reads as a closed
/// literal, and ` + &b` follows it). Every hit the arm could produce was of that shape — a false
/// positive, not coverage — so the arm is gone and this fixture keeps it gone.
#[test]
fn a_string_literal_with_escaped_quotes_beside_a_command_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/jobs.rs",
        "use std::process::Command;\npub fn run(name: &str) -> std::io::Result<std::process::Output> {\n    let doc = \"use \\\"a\\\" + &b\";\n    Command::new(\"echo\").arg(doc).arg(name).output()\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "command-and-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn the_command_and_interpolation_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/jobs.rs",
        "use std::process::Command;\npub fn run(period: &str) -> std::io::Result<std::process::Output> {\n    // zzop-command-and-interpolation-ok: `period` is validated against a fixed allow-list upstream\n    let script = format!(\"/usr/local/bin/report --period {}\", period);\n    Command::new(\"sh\").arg(\"-c\").arg(script).output()\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "command-and-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- hardcoded-secret, `rust-str-const` arm ---

#[test]
fn a_rust_typed_str_const_secret_is_flagged_by_the_rust_str_const_arm() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/config.rs",
        "pub const API_KEY: &str = \"a7Fk29QmZx41Lp08Wd\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "hardcoded-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
    assert_eq!(label_of(h[0]), Some("rust-str-const"));
}

/// The untyped Rust binding goes through the ORIGINAL `assignment` arm, not the new one — so the two
/// arms are pinned as covering different shapes rather than one shadowing the other.
#[test]
fn an_untyped_rust_let_binding_still_goes_through_the_assignment_arm() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/config.rs",
        "pub fn key() -> String {\n    let api_key = \"a7Fk29QmZx41Lp08Wd\";\n    api_key.to_string()\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "hardcoded-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
    assert_eq!(label_of(h[0]), Some("assignment"));
}

#[test]
fn a_rust_str_const_read_from_the_environment_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/config.rs",
        "pub fn api_key() -> String {\n    std::env::var(\"ZZOP_API_KEY\").unwrap_or_default()\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The nearest benign lookalike for the new arm: a typed `&str` constant whose name is secret-shaped but
/// whose value is an identifier, not a credential. The rule's pre-existing value-shape veto is what has
/// to catch this, and it has to keep catching it through the new arm too.
#[test]
fn a_typed_str_const_holding_an_identifier_shaped_value_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/config.rs",
        "pub const TOKEN_HEADER: &str = \"x-service-token\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}
