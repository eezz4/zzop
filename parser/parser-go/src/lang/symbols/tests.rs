use super::*;

fn sym<'a>(syms: &'a [SourceSymbol], name: &str) -> &'a SourceSymbol {
    syms.iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name:?} in {syms:?}"))
}

#[test]
fn top_level_function_symbol() {
    let src = "package main\n\nfunc DoThing() {\n\tx := 1\n\t_ = x\n}\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "DoThing");
    assert_eq!(s.kind, SourceSymbolKind::Function);
    assert!(s.exported);
    assert_eq!(s.line, 3);
    assert_eq!(s.body_start, Some(4));
    assert_eq!(s.body_end, Some(5));
    assert_eq!(s.id, "a.go#DoThing");
    assert_eq!(s.file, "a.go");
}

#[test]
fn unexported_function_symbol() {
    let src = "package main\n\nfunc doThing() {}\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "doThing");
    assert!(!s.exported);
    assert_eq!(s.body_start, None);
    assert_eq!(s.body_end, None);
}

#[test]
fn method_with_pointer_receiver() {
    let src = "package main\n\ntype Server struct{}\n\nfunc (s *Server) Start() {\n\trun()\n}\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "Server.Start");
    assert_eq!(s.kind, SourceSymbolKind::Function);
    assert!(s.exported);
    assert_eq!(s.line, 5);
    assert_eq!(s.body_start, Some(6));
}

#[test]
fn method_with_value_receiver() {
    let src = "package main\n\ntype Point struct{}\n\nfunc (p Point) x() int { return 0 }\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "Point.x");
    assert!(!s.exported); // lowercase method name
    assert_eq!(s.line, 5);
}

#[test]
fn method_with_generic_receiver_is_skipped() {
    let src = "package main\n\ntype Box[T any] struct{}\n\nfunc (b *Box[T]) Get() T {\n\tvar zero T\n\treturn zero\n}\n";
    let syms = parse_symbols("a.go", src);
    assert!(syms.iter().all(|s| !s.name.contains("Get")));
}

#[test]
fn struct_and_interface_and_type_alias_kinds() {
    let src = "package main\n\ntype User struct {\n\tName string\n}\n\ntype Reader interface {\n\tRead() error\n}\n\ntype ID int\n\ntype Alias = string\n";
    let syms = parse_symbols("a.go", src);
    assert_eq!(sym(&syms, "User").kind, SourceSymbolKind::Class);
    assert_eq!(sym(&syms, "Reader").kind, SourceSymbolKind::Interface);
    assert_eq!(sym(&syms, "ID").kind, SourceSymbolKind::Type);
    assert_eq!(sym(&syms, "Alias").kind, SourceSymbolKind::Type);
}

#[test]
fn grouped_type_declaration_emits_one_symbol_per_spec() {
    let src = "package main\n\ntype (\n\tX struct{}\n\tY interface{}\n)\n";
    let syms = parse_symbols("a.go", src);
    assert_eq!(sym(&syms, "X").kind, SourceSymbolKind::Class);
    assert_eq!(sym(&syms, "Y").kind, SourceSymbolKind::Interface);
    assert_eq!(sym(&syms, "X").line, 4);
    assert_eq!(sym(&syms, "Y").line, 5);
}

#[test]
fn ungrouped_const_is_const_kind() {
    let src = "package main\n\nconst MaxRetries = 3\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "MaxRetries");
    assert_eq!(s.kind, SourceSymbolKind::Const);
    assert!(s.exported);
    assert_eq!(s.line, 3);
}

#[test]
fn grouped_const_declaration_one_symbol_per_spec() {
    let src = "package main\n\nconst (\n\tA = 1\n\tb = 2\n)\n";
    let syms = parse_symbols("a.go", src);
    assert_eq!(sym(&syms, "A").line, 4);
    assert_eq!(sym(&syms, "b").line, 5);
    assert!(sym(&syms, "A").exported);
    assert!(!sym(&syms, "b").exported);
}

#[test]
fn const_spec_with_multiple_names_emits_one_symbol_each() {
    let src = "package main\n\nconst A, b = 1, 2\n";
    let syms = parse_symbols("a.go", src);
    assert_eq!(sym(&syms, "A").line, 3);
    assert_eq!(sym(&syms, "b").line, 3);
}

#[test]
fn top_level_var_maps_to_const_kind() {
    let src = "package main\n\nvar Counter = 0\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "Counter");
    assert_eq!(s.kind, SourceSymbolKind::Const);
    assert!(s.exported);
}

#[test]
fn grouped_var_declaration_via_var_spec_list() {
    let src = "package main\n\nvar (\n\tHost string\n\tport int\n)\n";
    let syms = parse_symbols("a.go", src);
    assert_eq!(sym(&syms, "Host").line, 4);
    assert_eq!(sym(&syms, "port").line, 5);
    assert!(sym(&syms, "Host").exported);
    assert!(!sym(&syms, "port").exported);
}

/// Regression pin: a function whose LAST top-level statement is itself multi-line (here, a `for` loop
/// that is the function's ONLY statement) must have `body_end` at the statement's own closing line, not
/// its opening line — module doc's "`body_start`/`body_end`" section. Before this fix, `body_end` used
/// `line_of` (the child's START line) for the last child too, so a function shaped `func f() { for
/// ... { ... } }` reported a `body_end` equal to `body_start`, silently excluding the loop's own body
/// lines from `Matcher::MethodScan`'s scan window — exactly the shape `trigger_in_loop` most needs to see.
#[test]
fn multiline_last_statement_body_end_is_its_own_closing_line() {
    let src = "package main\n\nfunc f(items []int) {\n\tfor _, it := range items {\n\t\tgo process(it)\n\t}\n}\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "f");
    assert_eq!(s.body_start, Some(4));
    assert_eq!(s.body_end, Some(6));
}

#[test]
fn empty_body_function_has_no_body_range() {
    let src = "package main\n\nfunc noop() {}\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "noop");
    assert_eq!(s.body_start, None);
    assert_eq!(s.body_end, None);
}

#[test]
fn parse_symbols_empty_on_hopeless_input() {
    assert!(parse_symbols("a.go", "@@@ ### not go").is_empty());
}

#[test]
fn parse_symbols_skips_broken_top_level_item_but_keeps_valid_ones() {
    // A malformed second top-level item must not blank out the first, valid one.
    let src = "package main\n\nfunc Good() {}\n\nfunc &&& broken\n";
    let syms = parse_symbols("a.go", src);
    assert!(syms.iter().any(|s| s.name == "Good"));
}
