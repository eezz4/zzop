// DECOY for sql/select-star. In scope: `.ts`, not a test path, and the rule's require_file
// (`SELECT\s+\*\s+FROM`) IS satisfied — by the house-rule comment below, so the rule really did scan this
// file. Its line_pattern additionally demands the star form appear INSIDE a string literal, and every
// query here names its columns.
// house rule: never write SELECT * FROM ledger_entries in application code.
export const LIVE_ROWS = 'SELECT id, amount_cents, opened_at FROM ledger_entries WHERE open = true';
export const ONE_ROW = 'SELECT id, amount_cents FROM ledger_entries WHERE id = $1';
export const COUNT_ROWS = 'SELECT count(id) FROM ledger_entries';
