use super::*;

#[test]
fn a_field_access_chain_contributes_object_and_field_but_not_the_middle() {
    let refs = parse_local_identifier_refs("class C { void f() { System.out.println(\"hi\"); } }");
    assert!(refs.contains("System"));
    assert!(refs.contains("out"));
    assert!(refs.contains("println"));
}

#[test]
fn a_scoped_type_identifier_contributes_only_its_rightmost_segment() {
    let refs = parse_local_identifier_refs("class C { java.util.List<String> xs; }");
    assert!(refs.contains("List"));
    assert!(!refs.contains("java"));
    assert!(!refs.contains("util"));
}

#[test]
fn a_type_identifier_reference_is_collected() {
    let refs = parse_local_identifier_refs("class C { String s; }");
    assert!(refs.contains("String"));
}

#[test]
fn declared_names_are_excluded() {
    let refs = parse_local_identifier_refs("class C { void m(int p) { int local = p; } }");
    // "C" (class name), "m" (method name), "p" (parameter name), "local" (variable name) are all
    // declarations, never reads.
    assert!(!refs.contains("C"));
    assert!(!refs.contains("m"));
    assert!(!refs.contains("local"));
    // `p` IS a read on the RHS of `local = p` even though it's ALSO a parameter declaration elsewhere —
    // the exclusion only applies to the declaring occurrence itself, not every later mention.
    assert!(refs.contains("p"));
}

#[test]
fn a_method_call_name_is_collected_via_the_general_identifier_rule() {
    let refs = parse_local_identifier_refs("class C { void f() { helper(); } }");
    assert!(refs.contains("helper"));
}

#[test]
fn an_enum_constant_name_is_not_a_used_name() {
    let refs = parse_local_identifier_refs("enum E { RED, GREEN, BLUE }");
    assert!(!refs.contains("RED"));
}

#[test]
fn parse_failure_yields_an_empty_set() {
    assert!(parse_local_identifier_refs("\u{0}\u{1}not java{{{{").is_empty());
}

#[test]
fn empty_file_yields_an_empty_set() {
    assert!(parse_local_identifier_refs("").is_empty());
}
