use super::*;

#[test]
fn plain_using_is_keyed_by_full_specifier() {
    let map = parse_imports("using System.Text;\n");
    let b = map.get("System.Text").unwrap();
    assert_eq!(b.specifier, "System.Text");
    assert_eq!(b.original, "Text");
    assert!(!b.deferred);
    assert!(!b.type_only);
}

#[test]
fn same_suffix_usings_do_not_collide() {
    // C# legally permits two usings whose last segment matches; a last-segment key would drop one.
    let map = parse_imports("using App.Models;\nusing Vendor.Models;\n");
    assert_eq!(map.get("App.Models").unwrap().specifier, "App.Models");
    assert_eq!(map.get("Vendor.Models").unwrap().specifier, "Vendor.Models");
}

#[test]
fn global_using_behaves_like_plain_using() {
    let map = parse_imports("global using System;\n");
    let b = map.get("System").unwrap();
    assert_eq!(b.specifier, "System");
}

#[test]
fn aliased_using_binds_the_alias_name() {
    let map = parse_imports("using Sys = System.Text;\n");
    let b = map.get("Sys").unwrap();
    assert_eq!(b.specifier, "System.Text");
    assert_eq!(b.original, "Sys");
}

#[test]
fn aliased_using_of_a_generic_type_keeps_verbatim_text() {
    let map = parse_imports("using X = System.Collections.Generic.List<int>;\n");
    let b = map.get("X").unwrap();
    assert_eq!(b.specifier, "System.Collections.Generic.List<int>");
}

#[test]
fn static_using_binds_a_synthetic_key() {
    let map = parse_imports("using static System.Math;\n");
    assert_eq!(map.len(), 1);
    let (key, b) = map.iter().next().unwrap();
    assert!(key.starts_with("__static_import_"));
    assert_eq!(b.specifier, "System.Math");
    assert_eq!(b.original, "*");
}

#[test]
fn using_inside_namespace_block_is_collected() {
    let map = parse_imports("namespace Foo { using System.Text; class C {} }\n");
    assert!(map.contains_key("System.Text"));
}

#[test]
fn multiple_static_usings_get_distinct_keys() {
    let map = parse_imports("using static System.Math;\nusing static System.Linq.Enumerable;\n");
    assert_eq!(map.len(), 2);
}

#[test]
fn parse_imports_empty_on_parse_failure() {
    assert!(parse_imports("\u{0}\u{1}not csharp{{{{").is_empty());
}
