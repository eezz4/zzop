// DECOY for sql/delete-no-where + sql/update-no-where. In scope on both counts: `.ts`, not a test or
// migration path, and BOTH require_files are satisfied (`DELETE FROM` and `UPDATE <t> SET` each appear).
// Every statement carries a WHERE clause, which is what the rules' shared `sql-where-veto` exclude reads.
export const PURGE_ONE = 'DELETE FROM ledger_entries WHERE id = $1';
export const PURGE_OLD = 'DELETE FROM audit_log WHERE created_at < $1';
export const MARK_SEEN = 'UPDATE ledger_entries SET seen = true WHERE id = $1';
export const CLOSE_BATCH = 'UPDATE ledger_entries SET open = false WHERE opened_at < $1';
