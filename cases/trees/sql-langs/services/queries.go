// PRODUCTION TWIN of services/queries_test.go — the Go half of the same five literals the Python, C# and
// Java twins in this tree carry. Table names are tree-unique (`sqlx_`) so these literals cannot move
// another tree's expected set through the `db-table` channel.
package services

import "fmt"

// sql/delete-no-where — a closed literal holding a whole-table DELETE.
const PurgeSessions = "DELETE FROM sqlx_go_sessions"

const GoodPurgeSessions = "DELETE FROM sqlx_go_sessions WHERE expires_at < now()"

// sql/update-no-where — the same discipline, for UPDATE.
const ResetBalances = "UPDATE sqlx_go_accounts SET balance = 0"

const GoodResetBalances = "UPDATE sqlx_go_accounts SET balance = 0 WHERE closed_at IS NOT NULL"

// sql/truncate-in-app-code — TRUNCATE outside a migration directory.
const WipeAuditLog = "TRUNCATE TABLE sqlx_go_audit_log"

const GoodWipeAuditLog = "DELETE FROM sqlx_go_audit_log WHERE created_at < now() - interval '90 days'"

// sql/select-star — `SELECT *` inside a literal.
const AllUsers = "SELECT * FROM sqlx_go_users"

const GoodAllUsers = "SELECT id, email FROM sqlx_go_users"

// sql/like-leading-wildcard — a leading `%` cannot use a B-tree index prefix.
const SearchUsers = "SELECT id FROM sqlx_go_users WHERE name LIKE '%term'"

const GoodSearchUsers = "SELECT id FROM sqlx_go_users WHERE name LIKE 'term%'"

// --- Go quote-form evidence: which spellings the line-scan can and cannot reach ---

// A RAW STRING closes with a backtick, which the matchers already accept as a quote kind, so a
// single-line raw literal is the same shape and FIRES.
const RawDelete = `DELETE FROM sqlx_go_jobs`

// The disclosed residual, planted so it is measured rather than assumed: Go's raw string is the idiomatic
// way to write a multi-line statement, and the statement line then carries no quote at all. SILENT.
const RawBlock = `
	DELETE FROM sqlx_go_events
`

// `%s` in a Sprintf template is a placeholder, not a wildcard — the pattern arrives at runtime. SILENT.
func LikePlaceholder(term string) string {
	return fmt.Sprintf("SELECT id FROM sqlx_go_users WHERE name LIKE '%s'", term)
}

// The other side of that exclusion, so it cannot quietly become a blanket veto on `%`: here the leading
// `%` is a GENUINE wildcard escaped as `%%`, and the rule FIRES.
func LikeRealWildcard(term string) string {
	return fmt.Sprintf("SELECT id FROM sqlx_go_users WHERE name LIKE '%%%s%%'", term)
}
