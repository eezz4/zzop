use crate::{hits, scan, TempDir};

// --- cmd-injection (Java, moved here from the dissolved java-security pack) ---

#[test]
fn method_that_execs_and_concatenates_a_string_is_flagged_the_dvja_pingaction_pattern() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "PingAction.java",
        "public class C {\n  private void run() {\n    String[] cmd = { \"/bin/bash\", \"-c\", \"ping \" + getAddress() };\n    Runtime.getRuntime().exec(cmd);\n  }\n}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "cmd-injection");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    // `C.run`, not bare `run`: parser-java-21 type-qualifies every method symbol (`Type.method`),
    // matching the workspace-wide TS/Python/Rust convention — the old lexical crate's unqualified
    // names were the deviation, retired with it.
    assert_eq!(
        found[0]
            .data
            .as_ref()
            .and_then(|d| d.get("method"))
            .and_then(|m| m.as_str()),
        Some("C.run")
    );
    let snippet = found[0]
        .data
        .as_ref()
        .and_then(|d| d.get("snippet"))
        .and_then(|s| s.as_str())
        .unwrap_or_default();
    assert!(snippet.contains("ping"), "{snippet}");
}

#[test]
fn exec_with_a_constant_command_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "Const.java",
        "public class C { void r(){ Runtime.getRuntime().exec(\"ls -la\"); } }",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cmd-injection").is_empty(), "{:?}", out.findings);
}

#[test]
fn string_concatenation_in_a_method_that_never_execs_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "NoExec.java",
        "public class C { String g(String n){ return \"hello \" + n; } }",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cmd-injection").is_empty(), "{:?}", out.findings);
}

#[test]
fn exec_in_one_method_and_concat_in_a_sibling_method_is_not_flagged_method_scoped() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "Sibling.java",
        "public class C {\n  void a() { Runtime.getRuntime().exec(\"safe\"); }\n  String b(String x) { return \"msg \" + x; }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cmd-injection").is_empty(), "{:?}", out.findings);
}

#[test]
fn process_builder_plus_concatenation_is_flagged() {
    // Deliberately multi-line: a single-line class+method body would give both spans identical line
    // numbers, and "innermost span wins" dedup only drops a STRICTLY wider span, so both would double-count.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "Pb.java",
        "class C {\n  void r(String h) {\n    new ProcessBuilder(\"sh\", \"-c\", \"curl \" + h).start();\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "cmd-injection").len(), 1, "{:?}", out.findings);
}

// --- sql-taint / weak-crypto (Java line-scan rules, moved here from the dissolved java-security pack) ---

#[test]
fn sql_taint_still_fires_on_string_concatenated_query() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "C.java",
        "public class C {\n  void run(String login) {\n    Query q = em.createQuery(\"SELECT u FROM User u WHERE u.login = '\" + login + \"'\");\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "sql-taint").len(), 1, "{:?}", out.findings);
}

#[test]
fn sql_taint_fires_on_an_update_whose_set_clause_is_a_separate_concatenated_literal() {
    // Covers `"UPDATE " + tableName + " SET col = 1"`, where `SET` is a separate trailing literal — a
    // pattern requiring both keywords in ONE literal would miss every UPDATE built this way.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "C.java",
        "public class C {\n  void run(String tableName) {\n    String q = \"UPDATE \" + tableName + \" SET col = 1\";\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "sql-taint").len(), 1, "{:?}", out.findings);
}

#[test]
fn prose_strings_containing_the_word_update_are_not_sql_taint() {
    // A bare `UPDATE\b` anywhere in a literal would make logging/exception prose fire; the verb must OPEN
    // the SQL string (`"UPDATE " + table`), so none of these shapes may match.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "P.java",
        "public class P {\n  void run(String entityName, String version, String date, String field) {\n    String a = \"Failed to update \" + entityName;\n    String b = \"Checking for update \" + version;\n    String c = \"Last update: \" + date;\n    String d = \"Please update your \" + field + \" now\";\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "sql-taint").len(), 0, "{:?}", out.findings);
}

#[test]
fn weak_crypto_still_fires_on_md5_and_des() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "D.java",
        "MessageDigest md = MessageDigest.getInstance(\"MD5\");\nCipher.getInstance(\"DES/CBC/PKCS5Padding\");\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "weak-crypto").len(), 2, "{:?}", out.findings);
}

// --- suppress_marker coverage for the moved Java rules ---

#[test]
fn sql_taint_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "C.java",
        "public class C {\n  void run(String login) {\n    Query q = em.createQuery(\"SELECT u FROM User u WHERE u.login = '\" + login + \"'\"); // sql-taint-ok: login is server-generated, never user input\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "sql-taint").is_empty(), "{:?}", out.findings);
}

#[test]
fn weak_crypto_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "D.java",
        "MessageDigest md = MessageDigest.getInstance(\"MD5\"); // weak-crypto-ok: non-security checksum only\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "weak-crypto").is_empty(), "{:?}", out.findings);
}

#[test]
fn cmd_injection_ok_marker_directly_above_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "PingAction.java",
        "public class C {\n  private void run() {\n    // cmd-injection-ok: getAddress() is validated against an allow-list above\n    String[] cmd = { \"/bin/bash\", \"-c\", \"ping \" + getAddress() };\n    Runtime.getRuntime().exec(cmd);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cmd-injection").is_empty(), "{:?}", out.findings);
}

#[test]
fn sql_taint_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/test/java/com/example/CTest.java",
        "public class C {\n  void run(String login) {\n    Query q = em.createQuery(\"SELECT u FROM User u WHERE u.login = '\" + login + \"'\");\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "sql-taint").is_empty(), "{:?}", out.findings);
}
