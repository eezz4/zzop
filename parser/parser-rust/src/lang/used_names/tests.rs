use super::*;

fn refs(text: &str) -> BTreeSet<String> {
    parse_local_identifier_refs(text)
}

#[test]
fn collects_a_read_reference() {
    let out = refs("const X: i32 = 1;\nfn f() {\n    println_stub(X);\n}\n");
    assert!(out.contains("X"));
    assert!(out.contains("println_stub"));
}

#[test]
fn function_declaration_name_is_excluded() {
    let out = refs("fn foo() {\n    bar();\n}\n");
    assert!(!out.contains("foo"));
    assert!(out.contains("bar"));
}

#[test]
fn struct_declaration_name_is_excluded() {
    let out = refs("struct Foo;\nfn f() {\n    let _ = Baz;\n}\n");
    assert!(!out.contains("Foo"));
    assert!(out.contains("Baz"));
}

#[test]
fn function_parameter_names_are_not_bindings_but_their_reads_are_references() {
    let out = refs("fn foo(a: i32, b: i32) -> i32 {\n    a + b\n}\n");
    assert!(out.contains("a"));
    assert!(out.contains("b"));
}

#[test]
fn let_binding_target_is_not_a_reference_but_its_type_and_init_reads_are() {
    let out = refs("fn f() {\n    let x: MyType = make();\n}\n");
    assert!(!out.contains("x"));
    assert!(out.contains("MyType"));
    assert!(out.contains("make"));
}

#[test]
fn qualified_call_keeps_only_the_last_segment() {
    let out = refs("fn f() {\n    Type::method();\n}\n");
    assert!(out.contains("method"));
    assert!(!out.contains("Type"));
}

#[test]
fn crate_path_keeps_only_the_last_segment() {
    let out = refs("fn f() {\n    let _ = crate::a::b::thing();\n}\n");
    assert!(out.contains("thing"));
    assert!(!out.contains("a"));
    assert!(!out.contains("b"));
}

#[test]
fn type_path_reference_in_a_signature_is_collected() {
    let out = refs("fn f(x: Widget) -> Gadget {\n    todo_stub()\n}\n");
    assert!(out.contains("Widget"));
    assert!(out.contains("Gadget"));
}

#[test]
fn match_arm_binding_reference_use() {
    let out = refs("fn f(v: i32) {\n    match v {\n        n => use_stub(n),\n    }\n}\n");
    assert!(out.contains("n"));
    assert!(out.contains("use_stub"));
}

#[test]
fn nested_expression_references_are_collected() {
    let out = refs("fn f() {\n    if a && b {\n        let c = [d, e];\n    }\n}\n");
    for name in ["a", "b", "d", "e"] {
        assert!(out.contains(name), "missing {name}: {out:?}");
    }
    assert!(!out.contains("c"));
}

#[test]
fn identifiers_only_inside_a_macro_call_are_not_collected() {
    // Out of v1 scope — macro args are opaque tokens to syn, not a structured Expr tree.
    let out = refs("fn f() {\n    my_macro!(hidden_name);\n}\n");
    assert!(!out.contains("hidden_name"));
}

#[test]
fn parse_failure_yields_empty_set() {
    assert!(refs("fn f(:\n").is_empty());
}

#[test]
fn empty_file_yields_empty_set() {
    assert!(refs("").is_empty());
}
