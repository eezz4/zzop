# PRODUCTION TWIN of services/test_queries.py — the Python half of the 2026-08-10 widening of the five
# quote-anchored `sql` rules. Every literal below is byte-identical in the other three production twins of
# this tree; the only thing that differs between the twins is the host language, which is the whole claim
# under test (the matchers carry no host-language syntax at all).
#
# Table names are tree-unique (`sqlx_`) for the reason trees/rust-svc/src/queries.rs records: a SQL
# literal becomes a `db-table` consume, and a plain `users` here would move another tree's expected set.


def purge_sessions():
    """sql/delete-no-where — a closed literal holding a whole-table DELETE."""
    return "DELETE FROM sqlx_py_sessions"


def good_purge_sessions():
    return "DELETE FROM sqlx_py_sessions WHERE expires_at < now()"


def reset_balances():
    """sql/update-no-where — the same discipline, for UPDATE."""
    return "UPDATE sqlx_py_accounts SET balance = 0"


def good_reset_balances():
    return "UPDATE sqlx_py_accounts SET balance = 0 WHERE closed_at IS NOT NULL"


def wipe_audit_log():
    """sql/truncate-in-app-code — TRUNCATE outside a migration directory."""
    return "TRUNCATE TABLE sqlx_py_audit_log"


def good_wipe_audit_log():
    return "DELETE FROM sqlx_py_audit_log WHERE created_at < now() - interval '90 days'"


def all_users():
    """sql/select-star — `SELECT *` inside a literal."""
    return "SELECT * FROM sqlx_py_users"


def good_all_users():
    return "SELECT id, email FROM sqlx_py_users"


def search_users():
    """sql/like-leading-wildcard — a leading `%` cannot use a B-tree index prefix."""
    return "SELECT id FROM sqlx_py_users WHERE name LIKE '%term'"


def good_search_users():
    return "SELECT id FROM sqlx_py_users WHERE name LIKE 'term%'"


# --- Python quote-form evidence: which spellings the line-scan can and cannot reach ---


def triple_quoted_one_line():
    """A single-line triple-quoted string still puts a quote character adjacent to the keyword, so it is
    the SAME shape the rule reads — it FIRES."""
    return """DELETE FROM sqlx_py_jobs"""


def triple_quoted_multi_line():
    """The disclosed residual, planted so it is measured rather than assumed: the statement sits on a line
    that carries no quote at all, and a line-scan reads one line. SILENT, and the rule's message says so.
    """
    return """
        DELETE FROM sqlx_py_events
    """


def like_percent_placeholder(term):
    """`%s` here is a printf placeholder, not a wildcard — the whole pattern arrives at runtime, so
    claiming a leading wildcard would be a claim about text this rule never sees. SILENT."""
    return "SELECT id FROM sqlx_py_users WHERE name LIKE '%s'" % term


def like_named_placeholder(term):
    """The named `%(x)s` form of the same thing. SILENT."""
    return "SELECT id FROM sqlx_py_users WHERE name LIKE '%(term)s'" % {"term": term}


def update_bound_parameter(cur):
    """A psycopg `%s` between the SET and the closing quote is a BOUND VALUE, exactly as `?` is — the
    statement is complete and still touches every row, so `sql/update-no-where` FIRES."""
    return cur.execute("UPDATE sqlx_py_flags SET active = %s", (0,))
