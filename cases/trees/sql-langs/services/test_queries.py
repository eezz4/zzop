# TEST TWIN of services/queries.py — the Python convention (`test_*.py`). Every finding the production
# twin carries is expected to be ABSENT here; any finding at all scores as a false positive (`benign` in
# EXPECTED.jsonc). Neither half proves anything alone — see trees/test-conventions/README.md for why.
#
# This is the half that proves the PREREQUISITE for the widening actually holds: before 2026-08-10 the
# DSL's shared test-path vocabulary knew only TypeScript's spellings, so admitting `.py` to three CRITICAL
# rules would have poured them into every Python test module full of SQL strings.


def test_purge_sessions():
    assert "DELETE FROM sqlx_py_sessions"


def test_reset_balances():
    assert "UPDATE sqlx_py_accounts SET balance = 0"


def test_wipe_audit_log():
    assert "TRUNCATE TABLE sqlx_py_audit_log"


def test_all_users():
    assert "SELECT * FROM sqlx_py_users"


def test_search_users():
    assert "SELECT id FROM sqlx_py_users WHERE name LIKE '%term'"
