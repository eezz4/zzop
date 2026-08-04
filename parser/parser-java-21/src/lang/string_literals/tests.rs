use zzop_core::{shannon_entropy_bits, value_hash_hex};

use super::extract_string_literals;

fn names_and_lines(src: &str) -> Vec<(String, u32)> {
    extract_string_literals("A.java", src)
        .into_iter()
        .map(|l| (l.name, l.line))
        .collect()
}

#[test]
fn fields_constants_and_locals_all_emit_via_the_declarator_arm() {
    let src = concat!(
        "class C {\n",
        "  private String password = \"hunter2secret\";\n",
        "  static final String API_KEY = \"abcd1234efgh5678\";\n",
        "  void m() {\n",
        "    String token = \"correct-horse-battery-staple\";\n",
        "  }\n",
        "}\n",
    );
    assert_eq!(
        names_and_lines(src),
        vec![
            ("password".to_string(), 2),
            ("API_KEY".to_string(), 3),
            ("token".to_string(), 5)
        ]
    );
}

#[test]
fn an_escaped_literal_is_silent_because_spelling_is_not_value() {
    // RED, measured pre-repair (2026-08-03 review): this used to emit ONE entry carrying the
    // SPELLING's hash (`value_hash_hex("a\\u002db")`) and the SPELLING's entropy — not the value's,
    // which is what `zzop_core::BoundStringLiteral`'s field doc promises and what the 80-bit
    // threshold was calibrated on (an escaped `"P…"` spelling of PlaceholderSecretValue measured
    // 120.9 bits vs the value's 75.7 — a false fire). Spelling ≠ value ⇒ never-guess silence.
    let src = "class C { String k = \"a\\u002db\"; }\n";
    assert!(
        extract_string_literals("A.java", src).is_empty(),
        "an escape-carrying literal must be silent, not hashed by its spelling"
    );
}

#[test]
fn an_escape_free_literal_keeps_the_exact_pre_gate_hash_and_bits() {
    // Regression pin for the escape gate: raw == cooked when no `\` appears, so the gate must be a
    // strict no-op here — byte-identical hash, bit-identical entropy.
    let src = "class C { String k = \"hunter2secret\"; }\n";
    let out = extract_string_literals("A.java", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].value_hash, value_hash_hex("hunter2secret"));
    assert_eq!(
        out[0].entropy.to_bits(),
        shannon_entropy_bits("hunter2secret").to_bits()
    );
}

#[test]
fn silent_shapes_emit_nothing() {
    // Text block, concatenation, plain assignment (not a declarator), annotation argument.
    let src = concat!(
        "class C {\n",
        "  String block = \"\"\"\n  body\n  \"\"\";\n",
        "  String cat = \"a\" + \"b\";\n",
        "  void m() { this.key = \"v\"; key2 = \"w\"; }\n",
        "  @SuppressWarnings(\"secret\") void n() {}\n",
        "}\n",
    );
    assert_eq!(names_and_lines(src), Vec::<(String, u32)>::new());
}

#[test]
fn an_unparseable_file_yields_nothing() {
    assert!(extract_string_literals("A.java", "%%%not java%%%").is_empty());
}
