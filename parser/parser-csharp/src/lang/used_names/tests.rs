use super::*;

fn refs(text: &str) -> BTreeSet<String> {
    parse_local_identifier_refs(text)
}

#[test]
fn method_body_reads_are_collected() {
    let src = "class C { void M() { var x = Helper.Compute(1); } }";
    let out = refs(src);
    assert!(out.contains("Compute"));
    assert!(out.contains("Helper"));
    // Declaring names excluded.
    assert!(!out.contains("C"));
    assert!(!out.contains("M"));
}

#[test]
fn qualified_name_type_reference_contributes_only_rightmost_segment() {
    // A dotted TYPE reference is a real `qualified_name` node (a distinct grammar shape from a
    // runtime member-access chain, module doc) — only its rightmost segment is collected.
    let src = "class C { void M() { System.Text.StringBuilder x = null; } }";
    let out = refs(src);
    assert!(out.contains("StringBuilder"));
    assert!(!out.contains("System"));
    assert!(!out.contains("Text"));
}

#[test]
fn member_access_expression_chain_collects_every_segment() {
    // Unlike a `qualified_name`, a runtime member-access chain (`System.Console.WriteLine(...)`) is
    // NESTED `member_access_expression` all the way down — no special-casing needed (module doc's
    // "free ride" note), so every segment is a genuine reference, mirroring
    // `zzop_parser_java_21::lang::used_names`'s identical behavior for a chained Java `field_access`.
    let src = "class C { void M() { System.Console.WriteLine(1); } }";
    let out = refs(src);
    assert!(out.contains("System"));
    assert!(out.contains("Console"));
    assert!(out.contains("WriteLine"));
}

#[test]
fn variable_declarator_name_excluded_but_type_reference_included() {
    let src = "class C { void M() { Foo x = null; } }";
    let out = refs(src);
    assert!(out.contains("Foo"));
    assert!(!out.contains("x"));
}

#[test]
fn using_alias_name_excluded() {
    let src = "using Sys = System.Text;\nclass C {}\n";
    let out = refs(src);
    assert!(!out.contains("Sys"));
    assert!(out.contains("Text"));
}

#[test]
fn parameter_name_excluded_but_parameter_type_included() {
    let src = "class C { void M(Foo bar) {} }";
    let out = refs(src);
    assert!(out.contains("Foo"));
    assert!(!out.contains("bar"));
}

#[test]
fn empty_on_parse_failure() {
    assert!(refs("\u{0}\u{1}not csharp{{{{").is_empty());
}
