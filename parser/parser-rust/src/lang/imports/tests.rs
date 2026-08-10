use super::*;

fn binding<'a>(map: &'a ImportMap, local: &str) -> &'a ImportBinding {
    map.get(local)
        .unwrap_or_else(|| panic!("no binding for {local:?} in {map:?}"))
}

#[test]
fn plain_use_path_binds_the_last_segment() {
    let map = parse_imports("use a::b::c;\n");
    let b = binding(&map, "c");
    assert_eq!(b.specifier, "a::b::c");
    assert_eq!(b.original, "c");
    assert!(!b.deferred && !b.type_only);
}

#[test]
fn crate_prefixed_use_keeps_the_crate_head() {
    let map = parse_imports("use crate::routes::handler;\n");
    let b = binding(&map, "handler");
    assert_eq!(b.specifier, "crate::routes::handler");
}

#[test]
fn super_prefixed_use_keeps_the_super_head() {
    let map = parse_imports("use super::shared;\n");
    let b = binding(&map, "shared");
    assert_eq!(b.specifier, "super::shared");
}

#[test]
fn self_prefixed_use_keeps_the_self_head() {
    let map = parse_imports("use self::helper;\n");
    let b = binding(&map, "helper");
    assert_eq!(b.specifier, "self::helper");
}

#[test]
fn renamed_use_binds_the_alias_and_keeps_the_original() {
    let map = parse_imports("use a::b::c as d;\n");
    let bnd = binding(&map, "d");
    assert_eq!(bnd.specifier, "a::b::c");
    assert_eq!(bnd.original, "c");
    assert!(!map.contains_key("c"));
}

#[test]
fn grouped_use_tree_binds_every_member() {
    let map = parse_imports("use a::{b, c as d};\n");
    assert_eq!(binding(&map, "b").specifier, "a::b");
    assert_eq!(binding(&map, "d").specifier, "a::c");
    assert_eq!(binding(&map, "d").original, "c");
}

#[test]
fn nested_grouped_use_tree() {
    let map = parse_imports("use a::{b::{c, d}, e};\n");
    assert_eq!(binding(&map, "c").specifier, "a::b::c");
    assert_eq!(binding(&map, "d").specifier, "a::b::d");
    assert_eq!(binding(&map, "e").specifier, "a::e");
}

#[test]
fn glob_import_gets_a_synthetic_key() {
    let map = parse_imports("use a::b::*;\n");
    assert_eq!(map.len(), 1);
    let (_, b) = map.iter().next().unwrap();
    assert_eq!(b.specifier, "a::b");
    assert_eq!(b.original, "*");
}

#[test]
fn multiple_glob_imports_get_distinct_synthetic_keys() {
    let map = parse_imports("use a::*;\nuse b::*;\n");
    assert_eq!(map.len(), 2);
    let specifiers: Vec<&str> = map.values().map(|b| b.specifier.as_str()).collect();
    assert!(specifiers.contains(&"a"));
    assert!(specifiers.contains(&"b"));
}

#[test]
fn pub_use_is_recorded_as_an_ordinary_binding() {
    // No re-export flag in this crate's `ImportBinding` output — see module doc.
    let map = parse_imports("pub use crate::inner::Thing;\n");
    let b = binding(&map, "Thing");
    assert_eq!(b.specifier, "crate::inner::Thing");
}

#[test]
fn bodiless_mod_decl_is_an_import_edge_encoded_as_self() {
    let map = parse_imports("mod routes;\n");
    let b = binding(&map, "routes");
    assert_eq!(b.specifier, "self::routes");
    assert_eq!(b.original, "routes");
}

#[test]
fn mod_with_a_body_is_not_an_import_edge() {
    let map = parse_imports("mod inner {\n    fn f() {}\n}\n");
    assert!(!map.contains_key("inner"));
}

#[test]
fn external_crate_head_is_recorded_verbatim() {
    let map = parse_imports("use serde::Deserialize;\n");
    let b = binding(&map, "Deserialize");
    assert_eq!(b.specifier, "serde::Deserialize");
}

#[test]
fn use_nested_inside_a_function_body_is_out_of_v1_scope() {
    let map = parse_imports("fn f() {\n    use std::collections::HashMap;\n}\n");
    assert!(map.is_empty(), "{map:?}");
}

#[test]
fn parse_failure_yields_empty_map() {
    assert!(parse_imports("use (:\n").is_empty());
}

#[test]
fn empty_file_yields_empty_map() {
    assert!(parse_imports("").is_empty());
}

// --- `#[path = "..."]` module declarations (see module doc) ---

#[test]
fn path_attr_mod_carries_the_literal_not_the_module_name() {
    // The module is named `tests`; its file is not `tests.rs`. Encoding this as `self::tests` would
    // send the resolver after a file that does not exist — which is what used to happen.
    let map = parse_imports("#[cfg(test)]\n#[path = \"resolve_tests.rs\"]\nmod tests;\n");
    let b = map.get("tests").expect("tests binding");
    assert_eq!(b.specifier, "#path::resolve_tests.rs");
    assert_eq!(b.original, "tests");
}

#[test]
fn a_sibling_cfg_attribute_is_not_mistaken_for_the_path_attribute() {
    // `#[cfg(test)] #[path = ...]` is the dominant pairing in practice, and the attribute list order
    // is the author's choice — neither order may change what is read.
    let reversed = parse_imports("#[path = \"a/b.rs\"]\n#[cfg(test)]\nmod m;\n");
    assert_eq!(reversed.get("m").expect("m").specifier, "#path::a/b.rs");
    let cfg_only = parse_imports("#[cfg(test)]\nmod m;\n");
    assert_eq!(cfg_only.get("m").expect("m").specifier, "self::m");
}

#[test]
fn a_mod_with_no_path_attribute_keeps_the_convention_specifier() {
    let map = parse_imports("mod plain;\n");
    assert_eq!(map.get("plain").expect("plain").specifier, "self::plain");
}

#[test]
fn a_path_attribute_shape_this_parser_does_not_understand_falls_back_to_convention() {
    // Never-guess: an unrecognized attribute form leaves the declaration on the convention path
    // rather than inventing a target from it.
    let map = parse_imports("#[path]\nmod m;\n");
    assert_eq!(map.get("m").expect("m").specifier, "self::m");
}

#[test]
fn a_mod_with_a_body_is_still_not_an_edge_even_with_a_path_attribute() {
    // An inline body means the contents are in THIS file; there is nothing to resolve. The attribute
    // does not change that, and this pins that the `content.is_none()` guard still runs first.
    assert!(parse_imports("#[path = \"x.rs\"]\nmod m { }\n").is_empty());
}
