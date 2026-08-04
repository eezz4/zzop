use zzop_core::value_hash_hex;

use super::extract_string_literals;

fn names_and_lines(src: &str) -> Vec<(String, u32)> {
    extract_string_literals("A.cs", src)
        .into_iter()
        .map(|l| (l.name, l.line))
        .collect()
}

#[test]
fn fields_consts_and_locals_all_emit_via_the_declarator_arm() {
    let src = concat!(
        "class C {\n",
        "  private string password = \"hunter2secret\";\n",
        "  const string ApiKey = \"abcd1234efgh5678\";\n",
        "  void M() {\n",
        "    var token = \"correct-horse-battery-staple\";\n",
        "  }\n",
        "}\n",
    );
    assert_eq!(
        names_and_lines(src),
        vec![
            ("password".to_string(), 2),
            ("ApiKey".to_string(), 3),
            ("token".to_string(), 5)
        ]
    );
}

#[test]
fn an_escaped_literal_is_silent_because_spelling_is_not_value() {
    // RED, measured pre-repair (2026-08-03 review): this used to emit the SPELLING's hash/entropy
    // (`value_hash_hex("a\\u002db")`) — see the Java twin's pin for the measured false-fire the
    // threshold calibration cannot survive. Spelling ≠ value ⇒ never-guess silence.
    let src = "class C { string k = \"a\\u002db\"; }\n";
    assert!(
        extract_string_literals("A.cs", src).is_empty(),
        "an escape-carrying literal must be silent, not hashed by its spelling"
    );
}

#[test]
fn an_escape_free_literal_keeps_the_exact_pre_gate_hash_and_bits() {
    let src = "class C { string k = \"hunter2secret\"; }\n";
    let out = extract_string_literals("A.cs", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].value_hash, value_hash_hex("hunter2secret"));
    assert_eq!(
        out[0].entropy.to_bits(),
        zzop_core::shannon_entropy_bits("hunter2secret").to_bits()
    );
}

#[test]
fn silent_shapes_emit_nothing() {
    // Interpolated, verbatim, concatenation, plain assignment, attribute argument, uninitialized.
    let src = concat!(
        "class C {\n",
        "  string i = $\"x{y}\";\n",
        "  string v = @\"verbatim\";\n",
        "  string c = \"a\" + \"b\";\n",
        "  string u;\n",
        "  void M() { this.key = \"v\"; }\n",
        "  [Obsolete(\"secret\")] void N() {}\n",
        "}\n",
    );
    assert_eq!(names_and_lines(src), Vec::<(String, u32)>::new());
}

#[test]
fn an_unparseable_file_yields_nothing() {
    assert!(extract_string_literals("A.cs", "%%%not csharp%%%").is_empty());
}
