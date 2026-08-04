use zzop_core::value_hash_hex;

use super::extract_string_literals;

fn names_and_lines(src: &str) -> Vec<(String, u32)> {
    extract_string_literals("a.rs", src)
        .into_iter()
        .map(|l| (l.name, l.line))
        .collect()
}

#[test]
fn let_const_static_and_impl_const_all_emit() {
    let src = concat!(
        "const API_KEY: &str = \"abcd1234efgh5678\";\n",
        "static TOKEN: &str = \"correct-horse-battery-staple\";\n",
        "struct S;\n",
        "impl S {\n",
        "    const SECRET: &'static str = \"hunter2secret\";\n",
        "}\n",
        "fn f() {\n",
        "    let password = \"p\";\n",
        "    let typed: &str = \"t\";\n",
        "}\n",
    );
    assert_eq!(
        names_and_lines(src),
        vec![
            ("API_KEY".to_string(), 1),
            ("TOKEN".to_string(), 2),
            ("SECRET".to_string(), 5),
            ("password".to_string(), 8),
            ("typed".to_string(), 9)
        ]
    );
}

#[test]
fn the_value_is_the_cooked_literal_raw_strings_included() {
    let src = "const A: &str = \"a\\x2db\";\nconst B: &str = r\"raw-value\";\n";
    let out = extract_string_literals("a.rs", src);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].value_hash, value_hash_hex("a-b"));
    assert_eq!(out[1].value_hash, value_hash_hex("raw-value"));
}

#[test]
fn a_cfg_test_region_still_emits_extraction_does_not_pre_judge_test_code() {
    // The per-rule `scan_test_regions` flag decides at eval; this producer must see test code
    // (module doc's "Test regions are NOT skipped here").
    let src = "#[cfg(test)]\nmod tests {\n    const KEY: &str = \"abcd1234efgh5678\";\n}\n";
    assert_eq!(names_and_lines(src), vec![("KEY".to_string(), 3)]);
}

#[test]
fn silent_shapes_emit_nothing() {
    let src = concat!(
        "fn f() {\n",
        "    let (a, b) = (\"x\", \"y\");\n",
        "    let owned = \"lit\".to_string();\n",
        "    let cat = concat!(\"a\", \"b\");\n",
        "    let bytes = b\"raw\";\n",
        "    let mut x = 1; x = 2;\n",
        "    let s = S { key: \"v\" };\n",
        "}\n",
    );
    assert_eq!(names_and_lines(src), Vec::<(String, u32)>::new());
}

#[test]
fn an_unparseable_file_yields_nothing() {
    assert!(extract_string_literals("a.rs", "fn = %%%").is_empty());
}
