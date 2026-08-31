use crate::{
    assert_disqualifier_clause_precedes_imperative, assert_landing_precedes_imperative, hits, scan,
    TempDir,
};

/// The identifier landing for `raw-query-unsafe-api`, spliced ahead of the tagged-template imperative.
///
/// WHY (`1.architecture/rules/rule-quality.md` §27 leg 3). The remedy and the reason the flagged API
/// exists are the SAME property: `$queryRaw` binds every `${...}`, and a bound parameter can only
/// stand where a VALUE can. So for the dominant reason a codebase reaches for `$queryRawUnsafe` — an
/// interpolated IDENTIFIER — the prescribed rewrite does not work, and the two failure modes are not
/// equally kind. A table name lands as `SELECT * FROM $1` and errors; an `ORDER BY` column is
/// ACCEPTED as a constant and the rows come back in an arbitrary order with nothing raised. The
/// message shipped at 294 characters and said neither.
///
/// NOT A DISQUALIFIER. The finding is still right for a reader whose interpolation is an identifier —
/// that string still reaches the planner unparameterized. What changes is that the one-line swap is
/// unavailable to them, and the message now says so before it offers the swap.
///
/// POSITION, not presence: the invalidation probe is to move this constant behind the imperative with
/// every token still spelled exactly once.
const IDENTIFIER_BINDING_LANDING: &str = "THE TAGGED TEMPLATE PARAMETERIZES EVERY `${...}`, WHICH IS THE SAME REASON THE UNSAFE CALL EXISTS: a bound parameter can stand only where the database expects a VALUE, so an interpolation that supplies an IDENTIFIER — a table name, the column in an `ORDER BY`, a schema picked per tenant — does not survive the rewrite.";

// --- raw-query-unsafe-api ---

/// POSITION pin on a DELIVERED finding, carrying both claims this message makes ahead of its remedy:
/// the harm clause it already had, and the landing added for §27 leg 3.
#[test]
fn the_identifier_landing_and_the_injection_clause_both_precede_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/reports.ts",
        "declare const prisma: any;\ndeclare const id: string;\nexport async function f() {\n  return prisma.$queryRawUnsafe(`SELECT * FROM users WHERE id = ${id}`);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "raw-query-unsafe-api");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let imperative = "Use `$queryRaw`/`$executeRaw` tagged templates";
    assert_disqualifier_clause_precedes_imperative(
        "raw-query-unsafe-api",
        &h[0].message,
        "any request-derived string interpolated into it is a SQL injection",
        imperative,
    );
    assert_landing_precedes_imperative(
        "raw-query-unsafe-api",
        &h[0].message,
        IDENTIFIER_BINDING_LANDING,
        imperative,
    );
    // The half a reader can act on. The silent arm is the load-bearing one: told only that the rewrite
    // "may not work", a reader assumes they would see an error, and for `ORDER BY` they would not.
    for needle in [
        "`SELECT * FROM $1`",
        "is ACCEPTED",
        "in whatever order the plan happened to produce",
        "a fixed map from the request's string to a literal you wrote",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/raw-query-unsafe-api: the landing lost {needle:?}: {}",
            h[0].message
        );
    }
    // `Prisma.raw` has to stay named as a re-opening of this finding, not as a third way out.
    let raw = h[0]
        .message
        .find("`Prisma.raw` around that fragment alone")
        .expect("the Prisma.raw exit left the message");
    assert!(
        h[0].message[raw..].contains("re-opens this finding"),
        "security/raw-query-unsafe-api: `Prisma.raw` is offered without its cost: {}",
        h[0].message
    );
}

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
