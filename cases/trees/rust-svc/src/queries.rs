// SQL statement literals, in Rust. Every rule exercised here is a LEXICAL scan over a string literal:
// the same regex that reads a `.ts` file reads this one, which is why widening `file_pattern` was the
// whole change and no Rust-specific arm was needed. `security/sql-format-interpolation` is the one
// exception — Rust does not build SQL with `"..." + ident`, it uses the `format!` macro, so that shape
// needed a rule of its own rather than an extension of the Java `security/sql-string-concat`.
// Table names are tree-unique on purpose (`svc_users`, not `users`): the SQL literals below become
// `db-table` CONSUMES, and MEASURED — a plain `users` here made `cross-layer/db-table-name-in-multiple-sources`
// fire in BOTH this tree and `api-be`, i.e. adding this tree silently changed another tree's expected set.
// This tree measures the per-file DSL axis; coupling it to the join axis by accident is not that.

/// sql/select-star — `SELECT *` inside a literal. The good form lists its columns.
pub fn all_users_sql() -> &'static str {
    "SELECT * FROM svc_users"
}

pub fn good_all_users_sql() -> &'static str {
    "SELECT id, email FROM svc_users"
}

/// sql/delete-no-where — a CLOSED literal holding a whole-table DELETE. The good form carries its WHERE
/// clause inside the same literal, which is exactly what the rule's veto reads.
pub fn purge_sessions_sql() -> &'static str {
    "DELETE FROM sessions"
}

pub fn good_purge_sessions_sql() -> &'static str {
    "DELETE FROM sessions WHERE expires_at < now()"
}

/// sql/update-no-where — the same discipline, for UPDATE.
pub fn reset_balances_sql() -> &'static str {
    "UPDATE accounts SET balance = 0"
}

pub fn good_reset_balances_sql() -> &'static str {
    "UPDATE accounts SET balance = 0 WHERE closed_at IS NOT NULL"
}

/// sql/truncate-in-app-code — TRUNCATE outside a migration directory.
pub fn wipe_audit_log_sql() -> &'static str {
    "TRUNCATE TABLE audit_log"
}
/// The good twin: the bounded retention DELETE app code should have written instead; its WHERE clause keeps `sql/delete-no-where` quiet too. Table is `svc_`-prefixed per the header's tree-unique rule, because unlike the TRUNCATE line this literal DOES become a `db-table` consume (the decoy tree also touches an `audit_log`).
pub fn good_wipe_audit_log_sql() -> &'static str { "DELETE FROM svc_audit_log WHERE created_at < now() - interval '90 days'" }
/// sql/like-leading-wildcard — a leading `%` cannot use a B-tree index prefix. The good form is a trailing-only wildcard, which can.
pub fn search_users_sql() -> &'static str {
    "SELECT id FROM svc_users WHERE name LIKE '%term'"
}

pub fn good_search_users_sql() -> &'static str {
    "SELECT id FROM svc_users WHERE name LIKE 'term%'"
}

/// security/sql-format-interpolation — the statement TEXT is assembled by `format!` around a `{}`
/// placeholder. The good form keeps the statement constant and leaves the value to a bound `$1`.
pub fn user_by_name_sql(name: &str) -> String {
    format!("SELECT id, email FROM svc_users WHERE name = '{}'", name)
}

pub fn good_user_by_name_sql() -> &'static str {
    "SELECT id, email FROM svc_users WHERE name = $1"
}

/// security/sql-format-interpolation ALONE — the partition pin. The `{}` sits between inner `'`
/// quotes inside the UPDATE template: `sql/update-no-where` used to read that inner `'` as the
/// literal's closing quote, hide the placeholder from its own exclusion, and co-fire CRITICAL on a
/// line whose value is spliced at runtime — two stories about one line. Same-kind quote termination
/// keeps this line the warning-severity interpolation rule's alone; a second label appearing here is
/// that partition breaking again. The good twin binds the value and carries its WHERE.
pub fn rename_user_sql(name: &str) -> String {
    format!("UPDATE svc_users SET name = '{}'", name)
}

pub fn good_rename_user_sql() -> &'static str {
    "UPDATE svc_users SET name = $1 WHERE id = $2"
}
