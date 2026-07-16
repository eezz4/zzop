//! Line-scan matcher tests: the be-security `sql-taint`/`weak-crypto` rules end-to-end, plus the
//! `exclude_pattern` (v2 #2), `file_exclude_pattern` (v3), and `require_file_absent` (v4) extensions.

use super::test_support::{label, rule_pack, scan, scan_pack, snippet};
use super::RulePackDef;

// --- sql-taint ---

#[test]
fn flags_sql_concatenated_with_variable() {
    let f = scan(
        r#"Query q = em.createQuery("SELECT u FROM User u WHERE u.login = '" + login + "'");"#,
        "C.java",
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 1);
    assert!(snippet(&f[0]).contains("createQuery"));
}

#[test]
fn does_not_flag_parameterized_query() {
    assert!(scan(
        r#"Query q = em.createQuery("SELECT u FROM User u WHERE u.login = :login");"#,
        "C.java",
    )
    .is_empty());
}

#[test]
fn does_not_flag_constant_concatenation() {
    assert!(scan(r#"String s = "SELECT a " + "FROM b";"#, "C.java").is_empty());
}

#[test]
fn does_not_flag_non_sql_concatenation() {
    assert!(scan(r#"log.info("user selected " + name);"#, "C.java").is_empty());
}

#[test]
fn does_not_flag_lone_keyword_in_prose() {
    assert!(scan(
        r#"throw new IllegalArgumentException("Cannot merge with object of type [" + parent + "]");"#,
        "C.java",
    )
    .is_empty());
}

#[test]
fn ignores_sql_in_comment() {
    assert!(scan(r#"// example: "SELECT * FROM t WHERE x=" + v"#, "C.java").is_empty());
}

#[test]
fn works_on_jsp_scriptlets() {
    let f = scan(
        r#"<% String q = "SELECT * FROM t WHERE id=" + request.getParameter("id"); %>"#,
        "x.jsp",
    );
    assert_eq!(f.len(), 1);
}

#[test]
fn flags_delete_concatenation() {
    assert_eq!(
        scan(
            r#"st.executeUpdate("DELETE FROM t WHERE id=" + id);"#,
            "C.java"
        )
        .len(),
        1
    );
}

// --- weak-crypto ---

#[test]
fn flags_digestutils_md5() {
    let f = scan(
        r#"return DigestUtils.md5DigestAsHex(password.getBytes());"#,
        "C.java",
    );
    assert_eq!(f.len(), 1);
    assert!(label(&f[0]).contains("weak hash"));
}

#[test]
fn flags_messagedigest_md5_and_sha1() {
    assert_eq!(
        scan(
            r#"MessageDigest md = MessageDigest.getInstance("MD5");"#,
            "C.java"
        )
        .len(),
        1
    );
    assert_eq!(
        scan(r#"MessageDigest.getInstance("SHA-1");"#, "C.java").len(),
        1
    );
}

#[test]
fn flags_weak_ciphers_and_ecb() {
    let des = scan(r#"Cipher.getInstance("DES/CBC/PKCS5Padding");"#, "C.java");
    assert!(label(&des[0]).contains("weak cipher"));
    let ecb = scan(r#"Cipher.getInstance("AES/ECB/PKCS5Padding");"#, "C.java");
    assert!(label(&ecb[0]).contains("ECB"));
}

#[test]
fn does_not_flag_strong_primitives() {
    assert!(scan(r#"MessageDigest.getInstance("SHA-256");"#, "C.java").is_empty());
    assert!(scan(r#"Cipher.getInstance("AES/GCM/NoPadding");"#, "C.java").is_empty());
    // 3DES is not single-DES
    assert!(scan(
        r#"Cipher.getInstance("DESede/CBC/PKCS5Padding");"#,
        "C.java"
    )
    .is_empty());
}

#[test]
fn ignores_weak_crypto_in_comments() {
    assert!(scan(
        r#"// legacy used DigestUtils.md5DigestAsHex here"#,
        "C.java"
    )
    .is_empty());
}

// --- extension #2: line-scan `exclude_pattern` ---

fn as_cast_pack() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bas\\b","exclude_pattern":"^\\s*import\\b"}}"#,
    )
}

#[test]
fn exclude_pattern_still_flags_a_plain_as_cast() {
    let f = scan_pack(&as_cast_pack(), "f.ts", "const x = y as Foo;\n", vec![]);
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn exclude_pattern_skips_import_alias_as() {
    let f = scan_pack(
        &as_cast_pack(),
        "f.ts",
        "import { useState as useLocalState } from \"react\";\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn exclude_pattern_only_skips_matching_lines_not_the_whole_file() {
    let f = scan_pack(
        &as_cast_pack(),
        "f.ts",
        "import { useState as useLocalState } from \"react\";\nconst x = y as Foo;\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 2);
}

// --- DSL v3 extension: `file_exclude_pattern` (line-scan) ---

fn exclude_pack_line_scan() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","file_exclude_pattern":"(^|/)scripts/","line_pattern":"\\bfoo\\("}}"#,
    )
}

#[test]
fn file_exclude_pattern_skips_a_matching_file_entirely_for_line_scan() {
    let f = scan_pack(
        &exclude_pack_line_scan(),
        "scripts/build.ts",
        "foo();\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn file_exclude_pattern_still_flags_a_non_matching_file_for_line_scan() {
    let f = scan_pack(&exclude_pack_line_scan(), "src/a.ts", "foo();\n", vec![]);
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn file_exclude_pattern_absent_does_not_change_line_scan_behavior() {
    let pack = rule_pack(
        r#"{"id":"r","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bfoo\\("}}"#,
    );
    let f = scan_pack(&pack, "scripts/build.ts", "foo();\n", vec![]);
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn file_exclude_pattern_bad_regex_skips_the_whole_line_scan_rule() {
    let pack = rule_pack(
        r#"{"id":"r","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","file_exclude_pattern":"(","line_pattern":"\\bfoo\\("}}"#,
    );
    let f = scan_pack(&pack, "src/a.ts", "foo();\n", vec![]);
    assert!(f.is_empty(), "{f:?}");
}

// --- DSL v4 extension: `require_file_absent` (line-scan) ---

fn require_file_absent_pack() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bsetInterval\\s*\\(","require_file_absent":["\\bclearInterval\\s*\\("]}}"#,
    )
}

#[test]
fn require_file_absent_fires_when_the_absent_pattern_is_missing_from_the_file() {
    let f = scan_pack(
        &require_file_absent_pack(),
        "f.ts",
        "const id = setInterval(tick, 1000);\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 1);
}

#[test]
fn require_file_absent_skips_the_file_when_the_absent_pattern_is_present_anywhere() {
    let f = scan_pack(
        &require_file_absent_pack(),
        "f.ts",
        "const id = setInterval(tick, 1000);\nfunction teardown() {\n  clearInterval(id);\n}\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn require_file_absent_empty_list_is_a_no_op() {
    let pack = rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bsetInterval\\s*\\("}}"#,
    );
    let f = scan_pack(
        &pack,
        "f.ts",
        "const id = setInterval(tick, 1000);\nclearInterval(id);\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn require_file_absent_bad_regex_skips_the_whole_rule() {
    let pack = rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bsetInterval\\s*\\(","require_file_absent":["("]}}"#,
    );
    let f = scan_pack(
        &pack,
        "f.ts",
        "const id = setInterval(tick, 1000);\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}
