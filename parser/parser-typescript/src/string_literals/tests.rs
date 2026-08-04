//! `extract_string_literals` coverage — the recognized binding shapes, and one negative per deliberate
//! silence the producer's module doc claims. In `string_literals/tests.rs` for the 300-line cap. Every positive compares the whole list (an over-emitting producer
//! is as wrong as a silent one).

use zzop_core::{shannon_entropy_bits, value_hash_hex};

use super::extract_string_literals;

fn names_and_lines(src: &str) -> Vec<(String, u32)> {
    extract_string_literals("a.ts", src)
        .into_iter()
        .map(|l| (l.name, l.line))
        .collect()
}

#[test]
fn const_let_var_declarations_emit_with_the_binding_name() {
    let src = "const apiKey = 'a';\nlet b = \"bb\";\nvar c = 'ccc';\n";
    assert_eq!(
        names_and_lines(src),
        vec![
            ("apiKey".to_string(), 1),
            ("b".to_string(), 2),
            ("c".to_string(), 3)
        ]
    );
}

#[test]
fn object_properties_emit_including_string_keys_and_nested_objects() {
    let src =
        "export const cfg = { apiKey: 'v1', \"client_secret\": 'v2', nested: { token: 'v3' } };\n";
    let names: Vec<String> = extract_string_literals("a.ts", src)
        .into_iter()
        .map(|l| l.name)
        .collect();
    assert_eq!(names, vec!["apiKey", "client_secret", "token"]);
}

#[test]
fn class_fields_emit_and_the_value_is_hashed_never_stored() {
    let src = "class C {\n  password = 'correct-horse-battery-staple';\n}\n";
    let out = extract_string_literals("a.ts", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].name, "password");
    assert_eq!(out[0].line, 2);
    assert_eq!(
        out[0].value_hash,
        value_hash_hex("correct-horse-battery-staple")
    );
    assert_eq!(
        out[0].entropy.to_bits(),
        shannon_entropy_bits("correct-horse-battery-staple").to_bits()
    );
}

#[test]
fn escapes_hash_as_the_cooked_value() {
    // `"a\x2db"` cooks to `a-b` — the string the program actually ships is what is hashed.
    let src = "const k = 'a\\x2db';\n";
    let out = extract_string_literals("a.ts", src);
    assert_eq!(out[0].value_hash, value_hash_hex("a-b"));
}

#[test]
fn nameless_and_non_literal_shapes_are_silent() {
    // Destructuring, computed key, shorthand, template literal, concatenation, positional argument,
    // member assignment: every one is a deliberate silence (module doc), not a guessed name.
    let src = concat!(
        "const { apiKey } = cfg;\n",
        "const o = { [k]: 'v', short };\n",
        "const t = `no-subst-template`;\n",
        "const cat = 'a' + 'b';\n",
        "use('positional');\n",
        "obj.assigned = 'v';\n",
    );
    assert_eq!(names_and_lines(src), Vec::<(String, u32)>::new());
}

#[test]
fn a_multiline_declaration_anchors_on_the_name_line() {
    let src = "const apiKey =\n  'correct-horse-battery-staple';\n";
    assert_eq!(names_and_lines(src), vec![("apiKey".to_string(), 1)]);
}

#[test]
fn an_unparseable_file_yields_nothing() {
    assert!(extract_string_literals("a.ts", "const = = {").is_empty());
}

#[test]
fn a_decorated_class_prop_emits_in_source_order_decorator_argument_first() {
    // RED, reproduced pre-repair (2026-08-03 review): swc's visitor walks a `ClassProp`'s KEY/VALUE
    // before its `decorators` field (AST struct order, not source order), so this emitted
    // [(password, 3), (example, 2)] — line 3 before line 2, violating the channel's source-order
    // determinism contract (the cache round trip pins emission order). The producer now sorts by
    // source offset before returning.
    let src = concat!(
        "class C {\n",
        "  @ApiProperty({ example: 'ex-value' })\n",
        "  password = 'correct-horse-battery-staple';\n",
        "}\n",
    );
    assert_eq!(
        names_and_lines(src),
        vec![("example".to_string(), 2), ("password".to_string(), 3)]
    );
}
