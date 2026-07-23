use super::*;

// One fixture per Go loop form the module doc names, plus a nested-loop ordering pin and a no-loop
// empty-result pin — mirrors `zzop_parser_typescript::loop_spans`'s own test shape.

#[test]
fn extract_loop_spans_classic_for_include_header() {
    let src = "package main\n\nfunc f() {\n\tfor i := 0; i < 10; i++ {\n\t\tdoThing()\n\t}\n}\n";
    let spans = extract_loop_spans("f.go", src);
    assert_eq!(spans, vec![(4, 6)]);
}

#[test]
fn extract_loop_spans_condition_only() {
    let src = "package main\n\nfunc f() {\n\tfor cond() {\n\t\tstep()\n\t}\n}\n";
    let spans = extract_loop_spans("f.go", src);
    assert_eq!(spans, vec![(4, 6)]);
}

#[test]
fn extract_loop_spans_infinite() {
    let src = "package main\n\nfunc f() {\n\tfor {\n\t\tstep()\n\t}\n}\n";
    let spans = extract_loop_spans("f.go", src);
    assert_eq!(spans, vec![(4, 6)]);
}

#[test]
fn extract_loop_spans_range() {
    let src =
        "package main\n\nfunc f(items []int) {\n\tfor _, it := range items {\n\t\tuse(it)\n\t}\n}\n";
    let spans = extract_loop_spans("f.go", src);
    assert_eq!(spans, vec![(4, 6)]);
}

#[test]
fn extract_loop_spans_nested_loops_emit_both_outer_first() {
    let src = "package main\n\nfunc f() {\n\tfor i := 0; i < 2; i++ {\n\t\tfor j := 0; j < 2; j++ {\n\t\t\tuse(i, j)\n\t\t}\n\t}\n}\n";
    let spans = extract_loop_spans("f.go", src);
    assert_eq!(spans, vec![(4, 8), (5, 7)]);
}

#[test]
fn extract_loop_spans_single_line_for_has_equal_start_end() {
    let src = "package main\n\nfunc f() {\n\tfor cond() { step() }\n}\n";
    let spans = extract_loop_spans("f.go", src);
    assert_eq!(spans, vec![(4, 4)]);
}

/// Regression pin (same defect CLASS as `lang::symbols::body_line_range`'s leading-comment bug): a loop
/// whose body opens with a `//` comment must still yield the correct loop span. Unlike `symbols`'
/// per-statement walk, `extract_loop_spans` records the `for_statement` NODE's own whole span (its `for`
/// keyword to its closing `}`), never inspecting the block's first/last named child — so an interior
/// leading comment can never move either boundary. This test proves that, rather than assuming it.
#[test]
fn extract_loop_spans_body_opening_with_comment_is_unaffected() {
    let src =
        "package main\n\nfunc f() {\n\tfor cond() {\n\t\t// leading comment\n\t\tstep()\n\t}\n}\n";
    let spans = extract_loop_spans("f.go", src);
    assert_eq!(spans, vec![(4, 7)]);
}

#[test]
fn extract_loop_spans_no_loop_yields_empty() {
    let src = "package main\n\nfunc f() int {\n\treturn 1\n}\n";
    assert!(extract_loop_spans("f.go", src).is_empty());
}
