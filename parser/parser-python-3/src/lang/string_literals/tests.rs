use zzop_core::{shannon_entropy_bits, value_hash_hex};

use super::extract_string_literals;

fn names_and_lines(src: &str) -> Vec<(String, u32)> {
    extract_string_literals("a.py", src)
        .into_iter()
        .map(|l| (l.name, l.line))
        .collect()
}

#[test]
fn module_class_and_function_level_assignments_emit() {
    let src =
        "API_KEY = 'a'\n\nclass C:\n    password = 'b'\n\n    def m(self):\n        token = 'c'\n";
    assert_eq!(
        names_and_lines(src),
        vec![
            ("API_KEY".to_string(), 1),
            ("password".to_string(), 4),
            ("token".to_string(), 7)
        ]
    );
}

#[test]
fn annotated_and_chained_assignments_emit_per_name_target() {
    let src = "secret: str = 'v'\na = b = 'w'\n";
    assert_eq!(
        names_and_lines(src),
        vec![
            ("secret".to_string(), 1),
            ("a".to_string(), 2),
            ("b".to_string(), 2)
        ]
    );
}

#[test]
fn implicit_concatenation_hashes_as_the_one_literal_python_defines_it_to_be() {
    let src = "key = 'ab' 'cd'\n";
    let out = extract_string_literals("a.py", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].value_hash, value_hash_hex("abcd"));
    assert_eq!(
        out[0].entropy.to_bits(),
        shannon_entropy_bits("abcd").to_bits()
    );
}

#[test]
fn silent_shapes_emit_nothing() {
    // Attribute target, tuple target, f-string, bytes, dict literal, keyword argument — each a
    // deliberate silence the module doc claims.
    let src = concat!(
        "self_like.key = 'v'\n",
        "a, b = 'x', 'y'\n",
        "f = f'interp{x}'\n",
        "raw = b'bytes'\n",
        "d = {'k': 'v'}\n",
        "call(kw='v')\n",
    );
    assert_eq!(names_and_lines(src), Vec::<(String, u32)>::new());
}

#[test]
fn an_unparseable_file_yields_nothing() {
    assert!(extract_string_literals("a.py", "def = (").is_empty());
}
