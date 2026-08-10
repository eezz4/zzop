// TEST TWIN of src/main/java/com/example/sqllangs/Queries.java. Every finding the production twin carries
// is expected to be ABSENT here; any finding at all scores as a false positive (`benign` in
// EXPECTED.jsonc).
//
// Stated rather than glossed: this twin's silence has TWO sufficient causes — the Maven `src/test/`
// DIRECTORY (a spelling the shared vocabulary has always had) and the `*Test.java` SUFFIX (one of the
// arms added 2026-08-10). It isolates neither, and it sits at the idiomatic location anyway, because the
// question this tree asks is "does the path gate hold for the newly-admitted languages", not "which of
// two arms holds". The C# twin next door has the same property and records it the same way.
package com.example.sqllangs;

public class QueriesTest {
    public static final String A = "DELETE FROM sqlx_java_sessions";
    public static final String B = "UPDATE sqlx_java_accounts SET balance = 0";
    public static final String C = "TRUNCATE TABLE sqlx_java_audit_log";
    public static final String D = "SELECT * FROM sqlx_java_users";
    public static final String E = "SELECT id FROM sqlx_java_users WHERE name LIKE '%term'";
}
