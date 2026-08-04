use zzop_core::value_hash_hex;

use super::extract_string_literals;

fn names_and_lines(src: &str) -> Vec<(String, u32)> {
    extract_string_literals("a.go", src)
        .into_iter()
        .map(|l| (l.name, l.line))
        .collect()
}

#[test]
fn const_var_and_short_declarations_emit() {
    let src = concat!(
        "package p\n",
        "const apiKey = \"abcd1234efgh5678\"\n",
        "var (\n",
        "\ttoken string = \"correct-horse-battery-staple\"\n",
        ")\n",
        "func f() {\n",
        "\tpassword := \"hunter2secret\"\n",
        "}\n",
    );
    assert_eq!(
        names_and_lines(src),
        vec![
            ("apiKey".to_string(), 2),
            ("token".to_string(), 4),
            ("password".to_string(), 7)
        ]
    );
}

#[test]
fn positional_pairing_emits_per_matching_position() {
    let src = "package p\nfunc f() {\n\ta, b := \"xx\", nonLiteral\n}\n";
    // `a` pairs with a literal, `b` with an identifier: one entry, position-local silence.
    assert_eq!(names_and_lines(src), vec![("a".to_string(), 3)]);
}

#[test]
fn an_escaped_interpreted_literal_is_silent_because_spelling_is_not_value() {
    // RED, measured pre-repair (2026-08-03 review): this used to emit the SPELLING's hash/entropy
    // (`value_hash_hex("a\\u002db")`) — see the Java twin's pin for the measured false-fire the
    // threshold calibration cannot survive. Spelling ≠ value ⇒ never-guess silence. Interpreted
    // literals ONLY: a raw literal's backslash IS its value (pin below).
    let src = "package p\nvar k = \"a\\u002db\"\n";
    assert!(
        extract_string_literals("a.go", src).is_empty(),
        "an escape-carrying interpreted literal must be silent, not hashed by its spelling"
    );
}

#[test]
fn an_escape_free_interpreted_literal_keeps_the_exact_pre_gate_hash_and_bits() {
    let src = "package p\nvar k = \"hunter2secret\"\n";
    let out = extract_string_literals("a.go", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].value_hash, value_hash_hex("hunter2secret"));
    assert_eq!(
        out[0].entropy.to_bits(),
        zzop_core::shannon_entropy_bits("hunter2secret").to_bits()
    );
}

#[test]
fn a_raw_string_literal_is_the_exact_value_backslash_included() {
    // Raw (backtick) literals have NO escape processing — a backslash there IS the value, so the
    // interpreted-only escape gate must not touch them.
    let src = "package p\nvar key = `raw-horse-battery`\nvar win = `C:\\temp`\n";
    let out = extract_string_literals("a.go", src);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].value_hash, value_hash_hex("raw-horse-battery"));
    assert_eq!(out[1].value_hash, value_hash_hex("C:\\temp"));
}

#[test]
fn silent_shapes_emit_nothing() {
    // Plain assignment, struct literal field, concatenation, mismatched pairing (multi-value call).
    let src = concat!(
        "package p\n",
        "func f() {\n",
        "\tx = \"v\"\n",
        "\tc := Config{Key: \"v\"}\n",
        "\tcat := \"a\" + \"b\"\n",
        "\tp, q := twoValues()\n",
        "\t_ = c; _ = cat; _ = p; _ = q\n",
        "}\n",
    );
    assert_eq!(names_and_lines(src), Vec::<(String, u32)>::new());
}

#[test]
fn an_unparseable_file_yields_nothing() {
    assert!(extract_string_literals("a.go", "%%%not go%%%").is_empty());
}

// --- the fielded-comma quirk: tree-sitter-go 0.25 puts a `name` FIELD on const_spec's commas ---

#[test]
fn a_multi_name_const_spec_emits_every_position() {
    // Reproduced defect: `children_by_field_name("name")` on a CONST spec yields the comma tokens
    // too (2k-1 nodes for k names — `var_spec` does not do this), so the count guard tripped and the
    // whole spec went silent. The identifier-kind filter is what pairs k names with k values again.
    let src =
        "package p\nconst apiKey, apiSecret = \"hunter2secret99\", \"correct-horse-battery\"\n";
    assert_eq!(
        names_and_lines(src),
        vec![("apiKey".to_string(), 2), ("apiSecret".to_string(), 2)]
    );
}

#[test]
fn a_multi_name_var_spec_still_emits_every_position() {
    // The regression pair: var specs carry NO fielded commas, so this already worked — pinned so the
    // const fix cannot regress the var path.
    let src = "package p\nvar tokenA, tokenB = \"xx12yz34\", \"yy56ab78\"\n";
    assert_eq!(
        names_and_lines(src),
        vec![("tokenA".to_string(), 2), ("tokenB".to_string(), 2)]
    );
}

#[test]
fn a_grouped_const_mixing_multi_and_single_name_specs_emits_all() {
    let src = concat!(
        "package p\n",
        "const (\n",
        "\ta, b = \"x1x2x3\", \"y1y2y3\"\n",
        "\tc    = \"z1z2z3\"\n",
        ")\n",
    );
    assert_eq!(
        names_and_lines(src),
        vec![
            ("a".to_string(), 3),
            ("b".to_string(), 3),
            ("c".to_string(), 4)
        ]
    );
}
