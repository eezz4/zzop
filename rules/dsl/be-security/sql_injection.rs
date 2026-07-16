use crate::{hits, scan, TempDir};

// --- raw-query-interpolation ---

#[test]
fn query_raw_unsafe_call_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/reports.ts",
        "declare const prisma: any;\ndeclare const id: string;\nexport async function f() {\n  return prisma.$queryRawUnsafe(`SELECT * FROM users WHERE id = ${id}`);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "raw-query-interpolation");
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
        hits(&out, "raw-query-interpolation").len(),
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
        hits(&out, "raw-query-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn raw_sql_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/reports.ts",
        "declare const prisma: any;\ndeclare const id: string;\nexport async function f() {\n  // raw-sql-ok: id is a validated internal UUID, never request-derived\n  return prisma.$queryRawUnsafe(`SELECT * FROM users WHERE id = ${id}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "raw-query-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- annotation-sql-concat (Java) ---

#[test]
fn jpa_query_annotation_with_string_concatenation_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/UserRepository.java",
        "public interface UserRepository {\n    @Query(\"SELECT u FROM User u WHERE u.name = '\" + name + \"'\")\n    User findByName(String name);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "annotation-sql-concat");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
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
        "public interface UserRepository {\n    // query-concat-ok: name is validated against an internal enum before this call\n    @Query(\"SELECT u FROM User u WHERE u.name = '\" + name + \"'\")\n    User findByName(String name);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "annotation-sql-concat").is_empty(),
        "{:?}",
        out.findings
    );
}
