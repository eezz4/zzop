use super::*;

#[test]
fn plain_import_binds_last_segment() {
    let src = "package main\n\nimport \"fmt\"\n";
    let map = parse_imports(src);
    let b = map.get("fmt").expect("binding");
    assert_eq!(b.specifier, "fmt");
    assert_eq!(b.original, "fmt");
    assert!(!b.deferred);
    assert!(!b.type_only);
}

#[test]
fn plain_import_multi_segment_path_binds_last_segment() {
    let src = "package main\n\nimport \"net/http\"\n";
    let map = parse_imports(src);
    let b = map.get("http").expect("binding");
    assert_eq!(b.specifier, "net/http");
    assert_eq!(b.original, "http");
}

#[test]
fn aliased_import_binds_alias() {
    let src = "package main\n\nimport nethttp \"net/http\"\n";
    let map = parse_imports(src);
    let b = map.get("nethttp").expect("binding");
    assert_eq!(b.specifier, "net/http");
    assert_eq!(b.original, "http");
    assert!(!map.contains_key("http"));
}

#[test]
fn dot_import_gets_synthetic_key_and_star_original() {
    let src = "package main\n\nimport . \"fmt\"\n";
    let map = parse_imports(src);
    let (_, b) = map
        .iter()
        .find(|(_, b)| b.specifier == "fmt")
        .expect("dot import present");
    assert_eq!(b.original, "*");
}

#[test]
fn blank_import_gets_synthetic_key_and_underscore_original() {
    let src = "package main\n\nimport _ \"database/sql\"\n";
    let map = parse_imports(src);
    let (_, b) = map
        .iter()
        .find(|(_, b)| b.specifier == "database/sql")
        .expect("blank import present");
    assert_eq!(b.original, "_");
}

#[test]
fn grouped_import_declaration_emits_one_entry_per_spec() {
    let src = "package main\n\nimport (\n\t\"fmt\"\n\tnethttp \"net/http\"\n\t. \"strings\"\n\t_ \"database/sql\"\n)\n";
    let map = parse_imports(src);
    assert_eq!(map.get("fmt").unwrap().specifier, "fmt");
    assert_eq!(map.get("nethttp").unwrap().specifier, "net/http");
    assert!(map
        .values()
        .any(|b| b.specifier == "strings" && b.original == "*"));
    assert!(map
        .values()
        .any(|b| b.specifier == "database/sql" && b.original == "_"));
    // 4 specs -> 4 entries, no collisions.
    assert_eq!(map.len(), 4);
}

#[test]
fn multiple_dot_imports_do_not_collide() {
    let src = "package main\n\nimport (\n\t. \"fmt\"\n\t. \"strings\"\n)\n";
    let map = parse_imports(src);
    assert_eq!(map.len(), 2);
    assert!(map.values().any(|b| b.specifier == "fmt"));
    assert!(map.values().any(|b| b.specifier == "strings"));
}

#[test]
fn specifier_is_verbatim() {
    let src = "package main\n\nimport \"github.com/acme/app/internal/db\"\n";
    let map = parse_imports(src);
    let b = map.get("db").expect("binding");
    assert_eq!(b.specifier, "github.com/acme/app/internal/db");
}

#[test]
fn parse_imports_empty_on_hopeless_input() {
    assert!(parse_imports("@@@ ### not go").is_empty());
}
