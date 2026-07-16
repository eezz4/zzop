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
