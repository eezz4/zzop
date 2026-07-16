use super::*;

fn binding<'a>(map: &'a ImportMap, local: &str) -> &'a ImportBinding {
    map.get(local)
        .unwrap_or_else(|| panic!("no binding for {local:?} in {map:?}"))
}

#[test]
fn plain_import_binds_the_rightmost_segment() {
    let map = parse_imports("import a.b.C;\n");
    let b = binding(&map, "C");
    assert_eq!(b.specifier, "a.b.C");
    assert_eq!(b.original, "C");
    assert!(!b.deferred && !b.type_only);
}

#[test]
fn single_segment_import_binds_itself() {
    let map = parse_imports("import C;\n");
    let b = binding(&map, "C");
    assert_eq!(b.specifier, "C");
    assert_eq!(b.original, "C");
}

#[test]
fn static_import_binds_the_rightmost_member_segment() {
    let map = parse_imports("import static a.b.C.m;\n");
    let b = binding(&map, "m");
    assert_eq!(b.specifier, "a.b.C.m");
    assert_eq!(b.original, "m");
}

#[test]
fn glob_import_gets_a_synthetic_key_and_asterisk_specifier() {
    let map = parse_imports("import a.b.*;\n");
    assert_eq!(map.len(), 1);
    let (key, b) = map.iter().next().unwrap();
    assert!(key.starts_with("__glob_import_"));
    assert_eq!(b.specifier, "a.b.*");
    assert_eq!(b.original, "*");
}

#[test]
fn static_glob_import_gets_the_same_synthetic_encoding() {
    let map = parse_imports("import static java.lang.Math.*;\n");
    assert_eq!(map.len(), 1);
    let (_, b) = map.iter().next().unwrap();
    assert_eq!(b.specifier, "java.lang.Math.*");
    assert_eq!(b.original, "*");
}

#[test]
fn multiple_imports_each_get_their_own_binding() {
    let map = parse_imports("import a.b.C;\nimport a.b.D;\n");
    assert_eq!(binding(&map, "C").specifier, "a.b.C");
    assert_eq!(binding(&map, "D").specifier, "a.b.D");
}

#[test]
fn multiple_glob_imports_get_distinct_synthetic_keys() {
    let map = parse_imports("import a.*;\nimport b.*;\n");
    assert_eq!(map.len(), 2);
    let specifiers: Vec<&str> = map.values().map(|b| b.specifier.as_str()).collect();
    assert!(specifiers.contains(&"a.*"));
    assert!(specifiers.contains(&"b.*"));
}

#[test]
fn package_declaration_is_not_an_import() {
    let map = parse_imports("package a.b;\nimport a.b.C;\n");
    assert_eq!(map.len(), 1);
    assert!(binding(&map, "C").specifier == "a.b.C");
}

#[test]
fn parse_failure_yields_empty_map() {
    assert!(parse_imports("\u{0}\u{1}not java{{{{").is_empty());
}

#[test]
fn empty_file_yields_empty_map() {
    assert!(parse_imports("").is_empty());
}
