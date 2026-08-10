// TEST TWIN of services/queries.go — the Go convention (`_test.go`). Every finding the production twin
// carries is expected to be ABSENT here; any finding at all scores as a false positive (`benign` in
// EXPECTED.jsonc). The literals are deliberately identical to the twin's, so a divergence can only come
// from the PATH.
package services

import "testing"

func TestQueries(t *testing.T) {
	_ = "DELETE FROM sqlx_go_sessions"
	_ = "UPDATE sqlx_go_accounts SET balance = 0"
	_ = "TRUNCATE TABLE sqlx_go_audit_log"
	_ = "SELECT * FROM sqlx_go_users"
	_ = "SELECT id FROM sqlx_go_users WHERE name LIKE '%term'"
}
