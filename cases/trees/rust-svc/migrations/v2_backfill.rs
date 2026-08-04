// WHOLE-FILE NEGATIVE CONTROL (cases/EXPECTED.jsonc `benign`): the fragment-alignment half of the
// 2026-08-03 `security/sql-format-interpolation` batch. `src/dynamic_queries.rs` proves the rule
// FIRES on interpolating statement templates; this file carries the same shape under `migrations/`,
// where the rule's `${test-paths-migrations}` exclusion — since 2026-08-03 the SAME vocabulary its
// `sql/` no-where siblings use, so the two sides of the placeholder partition also share one file
// set — must keep it silent: a migration interpolating an identifier into a schema statement is the
// canonical legitimate dynamic SQL, with nothing request-derived in reach.

pub fn backfill_sql(table: &str) -> String {
    format!("UPDATE {} SET migrated = 1", table)
}
