//! End-to-end coverage of `java-security.json` through `zzop_engine::analyze_tree` against real `.java`
//! files parsed off disk. `cmd-injection` needs a symbol's method-body span, supplied for `.java` sources
//! by `Language::JavaLexical`; `sql-taint`/`weak-crypto` (line-scan rules, no symbol spans needed) are
//! re-asserted here too.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Loads the real `rules/dsl/java-security/java-security.json` from the repo, filtered to just the
/// `java-security` pack.
fn java_security_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "java-security")
        .expect("java-security pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "java-security-fixture".to_string(),
        packs: vec![java_security_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("java-security/{rule}"))
        .collect()
}

// --- cmd-injection ---

#[test]
fn method_that_execs_and_concatenates_a_string_is_flagged_the_dvja_pingaction_pattern() {
    let dir = TempDir::new("zzop-java-security");
    dir.write(
        "PingAction.java",
        "public class C {\n  private void run() {\n    String[] cmd = { \"/bin/bash\", \"-c\", \"ping \" + getAddress() };\n    Runtime.getRuntime().exec(cmd);\n  }\n}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "cmd-injection");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(
        found[0]
            .data
            .as_ref()
            .and_then(|d| d.get("method"))
            .and_then(|m| m.as_str()),
        Some("run")
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
    let dir = TempDir::new("zzop-java-security");
    dir.write(
        "Const.java",
        "public class C { void r(){ Runtime.getRuntime().exec(\"ls -la\"); } }",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cmd-injection").is_empty(), "{:?}", out.findings);
}

#[test]
fn string_concatenation_in_a_method_that_never_execs_is_not_flagged() {
    let dir = TempDir::new("zzop-java-security");
    dir.write(
        "NoExec.java",
        "public class C { String g(String n){ return \"hello \" + n; } }",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cmd-injection").is_empty(), "{:?}", out.findings);
}

#[test]
fn exec_in_one_method_and_concat_in_a_sibling_method_is_not_flagged_method_scoped() {
    let dir = TempDir::new("zzop-java-security");
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
    let dir = TempDir::new("zzop-java-security");
    dir.write(
        "Pb.java",
        "class C {\n  void r(String h) {\n    new ProcessBuilder(\"sh\", \"-c\", \"curl \" + h).start();\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "cmd-injection").len(), 1, "{:?}", out.findings);
}

// --- sql-taint / weak-crypto (line-scan rules, unaffected by .java dispatch) ---

#[test]
fn sql_taint_still_fires_on_string_concatenated_query() {
    let dir = TempDir::new("zzop-java-security");
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
    let dir = TempDir::new("zzop-java-security");
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
    let dir = TempDir::new("zzop-java-security");
    dir.write(
        "P.java",
        "public class P {\n  void run(String entityName, String version, String date, String field) {\n    String a = \"Failed to update \" + entityName;\n    String b = \"Checking for update \" + version;\n    String c = \"Last update: \" + date;\n    String d = \"Please update your \" + field + \" now\";\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "sql-taint").len(), 0, "{:?}", out.findings);
}

#[test]
fn weak_crypto_still_fires_on_md5_and_des() {
    let dir = TempDir::new("zzop-java-security");
    dir.write(
        "D.java",
        "MessageDigest md = MessageDigest.getInstance(\"MD5\");\nCipher.getInstance(\"DES/CBC/PKCS5Padding\");\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "weak-crypto").len(), 2, "{:?}", out.findings);
}

// --- suppress_marker coverage ---

#[test]
fn sql_taint_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-java-security");
    dir.write(
        "C.java",
        "public class C {\n  void run(String login) {\n    Query q = em.createQuery(\"SELECT u FROM User u WHERE u.login = '\" + login + \"'\"); // sql-taint-ok: login is server-generated, never user input\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "sql-taint").is_empty(), "{:?}", out.findings);
}

#[test]
fn weak_crypto_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-java-security");
    dir.write(
        "D.java",
        "MessageDigest md = MessageDigest.getInstance(\"MD5\"); // weak-crypto-ok: non-security checksum only\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "weak-crypto").is_empty(), "{:?}", out.findings);
}

#[test]
fn cmd_injection_ok_marker_directly_above_suppresses_the_finding() {
    let dir = TempDir::new("zzop-java-security");
    dir.write(
        "PingAction.java",
        "public class C {\n  private void run() {\n    // cmd-injection-ok: getAddress() is validated against an allow-list above\n    String[] cmd = { \"/bin/bash\", \"-c\", \"ping \" + getAddress() };\n    Runtime.getRuntime().exec(cmd);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "cmd-injection").is_empty(), "{:?}", out.findings);
}

// --- test-path exclusion ---

#[test]
fn sql_taint_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-java-security");
    dir.write(
        "src/test/java/com/example/CTest.java",
        "public class C {\n  void run(String login) {\n    Query q = em.createQuery(\"SELECT u FROM User u WHERE u.login = '\" + login + \"'\");\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "sql-taint").is_empty(), "{:?}", out.findings);
}
