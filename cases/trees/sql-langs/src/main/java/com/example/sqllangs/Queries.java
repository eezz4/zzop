// PRODUCTION TWIN of src/test/java/com/example/sqllangs/QueriesTest.java — the Java half of the same five
// literals the Python, Go and C# twins in this tree carry. Table names are tree-unique (`sqlx_`) so these
// literals cannot move another tree's expected set through the `db-table` channel.
//
// The Java lane is the one with a pre-existing SQL neighbour: `security/sql-string-concat` already
// admitted `.java`. It judges a CONCATENATION shape (`"...select..." + ident`), and the three destructive
// rules here veto on exactly that concatenation, so those four partition the line rather than sharing it.
// `sql/select-star` is the one that CAN co-fire with it, and does — see
// trees/java-svc/src/main/java/com/example/svc/UnsafeController.java:19, which carries both labels: the
// over-fetch and the injection are two defects with two repairs, the same ruling weak-crypto and
// weak-password-hash carry on one credential line. Nothing is planted here to duplicate that measurement.
package com.example.sqllangs;

public class Queries {
    // sql/delete-no-where — a closed literal holding a whole-table DELETE.
    public static final String PURGE_SESSIONS = "DELETE FROM sqlx_java_sessions";

    public static final String GOOD_PURGE_SESSIONS = "DELETE FROM sqlx_java_sessions WHERE expires_at < now()";

    // sql/update-no-where — the same discipline, for UPDATE.
    public static final String RESET_BALANCES = "UPDATE sqlx_java_accounts SET balance = 0";

    public static final String GOOD_RESET_BALANCES = "UPDATE sqlx_java_accounts SET balance = 0 WHERE closed_at IS NOT NULL";

    // sql/truncate-in-app-code — TRUNCATE outside a migration directory.
    public static final String WIPE_AUDIT_LOG = "TRUNCATE TABLE sqlx_java_audit_log";

    public static final String GOOD_WIPE_AUDIT_LOG = "DELETE FROM sqlx_java_audit_log WHERE created_at < now() - interval '90 days'";

    // sql/select-star — `SELECT *` inside a literal.
    public static final String ALL_USERS = "SELECT * FROM sqlx_java_users";

    public static final String GOOD_ALL_USERS = "SELECT id, email FROM sqlx_java_users";

    // sql/like-leading-wildcard — a leading `%` cannot use a B-tree index prefix.
    public static final String SEARCH_USERS = "SELECT id FROM sqlx_java_users WHERE name LIKE '%term'";

    public static final String GOOD_SEARCH_USERS = "SELECT id FROM sqlx_java_users WHERE name LIKE 'term%'";

    // --- Java quote-form evidence: which spellings the line-scan can and cannot reach ---

    // The disclosed residual, planted so it is measured rather than assumed. A TEXT BLOCK is Java's
    // idiomatic multi-line SQL, and its opening `"""` sits on the line ABOVE the statement, so the
    // statement line carries no quote at all. SILENT — the rule's own message says so.
    public static final String TEXT_BLOCK = """
            DELETE FROM sqlx_java_events
            """;

    // `%s` in a String.format template is a placeholder, not a wildcard — the pattern arrives at runtime.
    // SILENT.
    public static String likePlaceholder(String term) {
        return String.format("SELECT id FROM sqlx_java_users WHERE name LIKE '%s'", term);
    }
}
