use super::*;

#[test]
fn file_scoped_namespace_is_reported() {
    assert_eq!(
        csharp_namespaces_of("namespace Foo.Bar;\nclass C {}\n"),
        vec!["Foo.Bar"]
    );
}

#[test]
fn block_namespace_is_reported() {
    assert_eq!(
        csharp_namespaces_of("namespace Foo.Bar { class C {} }\n"),
        vec!["Foo.Bar"]
    );
}

#[test]
fn nested_block_namespaces_each_get_a_fully_qualified_entry() {
    let ns = csharp_namespaces_of("namespace A { namespace B { class C {} } }\n");
    assert_eq!(ns, vec!["A".to_string(), "A.B".to_string()]);
}

#[test]
fn file_with_no_namespace_is_empty() {
    assert!(csharp_namespaces_of("class C {}\n").is_empty());
}

#[test]
fn empty_on_parse_failure() {
    assert!(csharp_namespaces_of("\u{0}\u{1}not csharp{{{{").is_empty());
}
