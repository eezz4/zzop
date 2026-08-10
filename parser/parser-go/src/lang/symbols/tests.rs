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
    assert_eq!(s.body_start, Some(3));
    assert_eq!(s.body_end, Some(6));
    assert_eq!(s.id, "a.go#DoThing");
    assert_eq!(s.file, "a.go");
}

#[test]
fn unexported_function_symbol() {
    let src = "package main\n\nfunc doThing() {}\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "doThing");
    assert!(!s.exported);
    // An empty body is still a declaration worth scanning — the contract's totality clause, pinned by
    // `empty_and_comment_only_bodies_still_report_the_declaration_span` below.
    assert_eq!(s.body_start, Some(3));
    assert_eq!(s.body_end, Some(3));
}

#[test]
fn method_with_pointer_receiver() {
    let src = "package main\n\ntype Server struct{}\n\nfunc (s *Server) Start() {\n\trun()\n}\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "Server.Start");
    assert_eq!(s.kind, SourceSymbolKind::Function);
    assert!(s.exported);
    assert_eq!(s.line, 5);
    assert_eq!(s.body_start, Some(5));
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
fn a_multi_name_const_spec_emits_exactly_its_names_and_no_comma_ghost() {
    // Reproduced defect (the fielded-comma quirk): tree-sitter-go 0.25 attaches the `name` FIELD to
    // a const_spec's comma tokens too, and this walk had no kind gate, so `const A, B = "x", "y"`
    // emitted a third symbol spelled "," (id `a.go#,`) into the graph/dead-export/count machinery.
    // The strict full-list assertion is the pin the finder-style test above cannot provide.
    let src = "package main\n\nconst A, B = \"x\", \"y\"\n";
    let names: Vec<String> = parse_symbols("a.go", src)
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(names, vec!["A".to_string(), "B".to_string()]);
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

/// Regression pin, kept from the 2026-08-02 `body_end` fix: a function whose LAST top-level statement
/// is itself multi-line (here, a `for` loop that is the function's ONLY statement) must not have that
/// statement's own lines fall outside the scan window — exactly the shape `trigger_in_loop` most needs
/// to see. The original bug took the last child's START line; the span is now anchored on the brace, so
/// the failure mode is structurally gone rather than patched, and this test is what says so.
#[test]
fn multiline_last_statement_is_fully_inside_the_body_span() {
    let src = "package main\n\nfunc f(items []int) {\n\tfor _, it := range items {\n\t\tgo process(it)\n\t}\n}\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "f");
    assert_eq!(s.body_start, Some(3));
    assert_eq!(s.body_end, Some(7));
}

/// The three comment shapes that each used to corrupt or erase a Go body span — a leading comment, one
/// between statements, and a trailing one — in ONE pin, because they no longer have separate causes.
/// `tree-sitter-go` treats `comment` as an "extra" that can be spliced in as a named child anywhere a
/// statement can appear, which is why walking into `statement_list` for the first/last real child was
/// fragile in three different places. The span no longer reads the block's contents at all.
#[test]
fn comments_anywhere_in_a_body_cannot_move_either_boundary() {
    for src in [
        "package main\n\nfunc f() {\n\t// leading comment\n\tx := 1\n\t_ = x\n}\n",
        "package main\n\nfunc f() {\n\tx := 1\n\t// mid comment\n\t_ = x\n}\n",
        "package main\n\nfunc f() {\n\tx := 1\n\t_ = x\n\t// trailing comment\n}\n",
    ] {
        let syms = parse_symbols("a.go", src);
        let s = sym(&syms, "f");
        assert_eq!(s.body_start, Some(3), "in:\n{src}");
        assert_eq!(s.body_end, Some(7), "in:\n{src}");
    }
}

// --- THE SPAN CONTRACT: `body_start` is the DECLARATION's line ---

/// `zzop_core::SourceSymbol`'s "Body span contract". Under the old first-statement reading a Go
/// method-scan rule could not see the `func` line at all, which is why the whole family of
/// declaration-anchored concepts that ship for TypeScript (`async` handlers, decorated routes) was
/// structurally unwritable for `.go` — the declaration is never inside any span. The wrapped signature
/// here is the shape that also breaks the brace-line reading, so this pins the contract rather than a
/// formatting coincidence.
#[test]
fn function_body_span_starts_at_the_declaration_and_ends_at_the_closing_brace() {
    let src = "package main\n\nfunc handler(\n\tw http.ResponseWriter,\n) {\n\tuse(w)\n}\n";
    let syms = parse_symbols("a.go", src);
    let s = sym(&syms, "handler");
    assert_eq!(s.line, 3);
    assert_eq!(s.body_start, Some(3));
    assert_eq!(s.body_end, Some(7));
}

/// The contract's totality clause: a body that holds no statement at all still has a declaration line,
/// so it reports a span. Under the first-statement reading an empty or comment-only body collapsed the
/// WHOLE symbol to `None`/`None` — a silent, total loss of scannability that had nothing to do with
/// whether the declaration was worth scanning. Only a genuinely body-LESS `func` (a `//go:linkname`
/// forward declaration, no braces at all) keeps `None`.
#[test]
fn empty_and_comment_only_bodies_still_report_the_declaration_span() {
    let src = "package main\n\nfunc noop() {}\n\nfunc todo() {\n\t// nothing but a comment\n}\n\nfunc decl()\n";
    let syms = parse_symbols("a.go", src);
    let noop = sym(&syms, "noop");
    assert_eq!(noop.body_start, Some(3));
    assert_eq!(noop.body_end, Some(3));
    let todo = sym(&syms, "todo");
    assert_eq!(todo.body_start, Some(5));
    assert_eq!(todo.body_end, Some(7));
    let decl = sym(&syms, "decl");
    assert_eq!(decl.body_start, None);
    assert_eq!(decl.body_end, None);
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
