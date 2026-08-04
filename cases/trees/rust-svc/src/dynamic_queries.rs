// security/sql-format-interpolation — the 2026-08-03 matcher: the template must BEGIN with the
// statement (leading whitespace aside), and the `{}` placeholder counts on EITHER side of the
// statement keywords. This file carries the two directions the original after-the-keyword matcher
// could not see, each beside its nearest benign lookalike, per this tree's convention that every bad
// shape has its `good_` twin. `queries.rs` keeps the original direction (`... WHERE name = '{}'`).

/// Placeholder BEFORE the `FROM` keyword — a dynamic column list, the classic non-parameterizable
/// vector. The good form keeps the projection constant and leaves nothing to interpolate.
pub fn users_projection_sql(cols: &str) -> String {
    format!("SELECT {} FROM svc_users", cols)
}

pub fn good_users_projection_sql() -> &'static str {
    "SELECT id, email FROM svc_users"
}

/// Placeholder in the TABLE position of an UPDATE — the statement's second keyword arrives after the
/// interpolation. The good form names its table and binds the value as a `$1` parameter.
pub fn touch_rows_sql(table: &str) -> String {
    format!("UPDATE {} SET touched_at = now()", table)
}

pub fn good_touch_rows_sql() -> &'static str {
    "UPDATE svc_sessions SET touched_at = now() WHERE id = $1"
}

/// The log-line lookalike the begins-with-a-statement anchor exists to keep silent: SQL keywords
/// mid-sentence, interpolation present, statement text never built. Before the anchor this exact
/// shape fired — it is a false positive the rule's message now names, not a statement.
pub fn cache_delete_error(key: &str) -> String {
    format!("failed to DELETE FROM cache for key {}", key)
}
