use crate::{hits, scan, TempDir};

// --- raw-query-unsafe-api ---

#[test]
fn query_raw_unsafe_call_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/reports.ts",
        "declare const prisma: any;\ndeclare const id: string;\nexport async function f() {\n  return prisma.$queryRawUnsafe(`SELECT * FROM users WHERE id = ${id}`);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "raw-query-unsafe-api");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn execute_raw_unsafe_call_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/admin.ts",
        "declare const prisma: any;\ndeclare const sql: string;\nexport async function f() {\n  return prisma.$executeRawUnsafe(sql);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "raw-query-unsafe-api").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn parameterized_execute_raw_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/admin.ts",
        "declare const prisma: any;\nexport async function f() {\n  return prisma.$executeRaw(`DELETE FROM sessions WHERE id = ${1}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "raw-query-unsafe-api").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn raw_sql_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/reports.ts",
        "declare const prisma: any;\ndeclare const id: string;\nexport async function f() {\n  // zzop-raw-query-unsafe-api-ok: id is a validated internal UUID, never request-derived\n  return prisma.$queryRawUnsafe(`SELECT * FROM users WHERE id = ${id}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "raw-query-unsafe-api").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- annotation-sql-concat (Java) ---

// Fixture note: the concatenated operand must be a `static final` String constant — a method
// parameter (`+ name`) in an annotation element value is not a constant expression and does not
// compile (JLS 9.7.1), and the rule's own message leans on exactly that language guarantee.
#[test]
fn jpa_query_annotation_with_string_concatenation_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/UserRepository.java",
        "public interface UserRepository {\n    static final String ROLE = \"admin\";\n    @Query(\"SELECT u FROM User u WHERE u.role = '\" + ROLE + \"'\")\n    User findAdmins();\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "annotation-sql-concat");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

/// The 2026-08-03 co-fire repair, negative direction: an annotation line whose concatenated SQL
/// literal is followed by an identifier used to fire BOTH rules — `annotation-sql-concat` saying
/// "injection is impossible here (JLS constant expression)" and `sql-string-concat` saying
/// "request-derived means injection" on the SAME line. The annotation-line `exclude_pattern` on
/// `sql-string-concat` resolves it: the annotation shape belongs to `annotation-sql-concat` alone.
#[test]
fn annotation_line_fires_only_the_annotation_rule_not_sql_string_concat() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/UserRepository.java",
        "public interface UserRepository {\n    static final String ROLE = \"admin\";\n    @Query(\"SELECT u FROM User u WHERE u.role = \" + ROLE)\n    User findAdmins();\n}\n",
    );
    let out = scan(&dir);
    let ann = hits(&out, "annotation-sql-concat");
    assert_eq!(ann.len(), 1, "{:?}", out.findings);
    assert_eq!(ann[0].line, 3);
    assert!(
        hits(&out, "sql-string-concat").is_empty(),
        "sql-string-concat must not co-fire on an annotation line: {:?}",
        out.findings
    );
}

/// Positive pair for the exclusion above: the same concatenation on an ORDINARY code line — where
/// nothing constrains the operand to a constant — still fires `sql-string-concat` (and never the
/// annotation rule).
#[test]
fn ordinary_code_line_concatenation_still_fires_sql_string_concat() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/UserDao.java",
        "public class UserDao {\n    String query(String role) {\n        return \"SELECT u FROM User u WHERE u.role = \" + role;\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-string-concat");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    assert!(
        hits(&out, "annotation-sql-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn jpa_query_annotation_with_named_parameter_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/UserRepository.java",
        "public interface UserRepository {\n    @Query(\"SELECT u FROM User u WHERE u.name = :name\")\n    User findByName(String name);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "annotation-sql-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn query_concat_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/UserRepository.java",
        "public interface UserRepository {\n    static final String ROLE = \"admin\";\n    // zzop-annotation-sql-concat-ok: constant-folded fragment, kept concatenated for line-length only\n    @Query(\"SELECT u FROM User u WHERE u.role = '\" + ROLE + \"'\")\n    User findAdmins();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "annotation-sql-concat").is_empty(),
        "{:?}",
        out.findings
    );
}
