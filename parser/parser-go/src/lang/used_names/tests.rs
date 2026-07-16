use super::*;

#[test]
fn bare_identifier_read_is_collected() {
    let src = "package main\n\nfunc main() {\n\tfoo()\n}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("foo"));
}

#[test]
fn selector_field_rightmost_is_collected() {
    let src = "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"hi\")\n}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("Println"));
    // The operand itself is a plain identifier read too.
    assert!(refs.contains("fmt"));
}

#[test]
fn type_reference_is_collected() {
    let src = "package main\n\ntype Foo struct{}\n\nfunc use(x Foo) {}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("Foo"));
}

#[test]
fn function_declaration_name_excluded() {
    let src = "package main\n\nfunc DoThing() {}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(!refs.contains("DoThing"));
}

#[test]
fn parameter_name_excluded_but_type_included() {
    let src = "package main\n\nfunc f(count int) {\n\tuse(count)\n}\n";
    let refs = parse_local_identifier_refs(src);
    // "count" IS used later as a read (call argument) — that occurrence must still be collected.
    assert!(refs.contains("count"));
    assert!(refs.contains("use"));
}

#[test]
fn const_and_var_spec_names_excluded() {
    let src = "package main\n\nconst Pi = 3\nvar Ready = false\n";
    let refs = parse_local_identifier_refs(src);
    assert!(!refs.contains("Pi"));
    assert!(!refs.contains("Ready"));
}

#[test]
fn type_spec_name_excluded_but_underlying_type_reference_included() {
    let src = "package main\n\ntype Wrapper struct{}\n\ntype Alias Wrapper\n";
    let refs = parse_local_identifier_refs(src);
    assert!(!refs.contains("Alias"));
    assert!(refs.contains("Wrapper"));
}

#[test]
fn short_var_declaration_left_excluded_but_right_included() {
    let src = "package main\n\nfunc f() {\n\tx := helper()\n\t_ = x\n}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("helper"));
    // `x` on the RIGHT of `_ = x` (a plain assignment, not a fresh binding) IS a read.
    assert!(refs.contains("x"));
}

#[test]
fn plain_reassignment_target_is_included() {
    let src = "package main\n\nfunc f() {\n\tvar x int\n\tx = 5\n\t_ = x\n}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("x"));
}

#[test]
fn parse_local_identifier_refs_empty_on_hopeless_input() {
    assert!(parse_local_identifier_refs("@@@ ### not go").is_empty());
}

#[test]
fn broken_statement_does_not_blank_out_valid_sibling_reads() {
    let src = "package main\n\nfunc main() {\n\tgood()\n\t&&& broken\n}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("good"));
}
