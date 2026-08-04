use super::*;

// One fixture per Rust loop form the module doc names, plus the iterator-adapter lazy-silence pin, a
// nested-loop ordering pin and a no-loop empty-result pin — mirrors `zzop_parser_go::lang::
// loop_spans`'s test shape.

#[test]
fn extract_loop_spans_for_include_header() {
    let src = "fn f() {\n    for i in 0..10 {\n        do_thing();\n    }\n}\n";
    let spans = extract_loop_spans("f.rs", src);
    assert_eq!(spans, vec![(2, 4)]);
}

#[test]
fn extract_loop_spans_while() {
    let src = "fn f() {\n    while cond() {\n        step();\n    }\n}\n";
    let spans = extract_loop_spans("f.rs", src);
    assert_eq!(spans, vec![(2, 4)]);
}

#[test]
fn extract_loop_spans_while_let() {
    let src = "fn f(mut it: std::vec::IntoIter<u32>) {\n    while let Some(x) = it.next() {\n        use_it(x);\n    }\n}\n";
    let spans = extract_loop_spans("f.rs", src);
    assert_eq!(spans, vec![(2, 4)]);
}

#[test]
fn extract_loop_spans_infinite_loop() {
    let src = "fn f() {\n    loop {\n        step();\n    }\n}\n";
    let spans = extract_loop_spans("f.rs", src);
    assert_eq!(spans, vec![(2, 4)]);
}

#[test]
fn extract_loop_spans_nested_loops_emit_outer_first() {
    let src = "fn f() {\n    for i in 0..2 {\n        for j in 0..2 {\n            use_it(i, j);\n        }\n    }\n}\n";
    let spans = extract_loop_spans("f.rs", src);
    assert_eq!(spans, vec![(2, 6), (3, 5)]);
}

/// The lazy-silence pin (module doc's contract boundary): an iterator adapter's closure runs zero
/// times unless the iterator is consumed — never a span, even when a `collect` sits right there (v1
/// never-guess: judging the chain's terminal would be a guess).
#[test]
fn extract_loop_spans_iterator_map_closure_is_silent() {
    let src = "fn f(xs: &[u32]) -> Vec<u32> {\n    xs.iter()\n        .map(|x| fetch(*x))\n        .collect()\n}\n";
    assert!(extract_loop_spans("f.rs", src).is_empty());
}

/// A method in an `impl` block is walked too — loops are not a free-function-only projection.
#[test]
fn extract_loop_spans_loop_inside_impl_method() {
    let src = "struct S;\nimpl S {\n    fn m(&self) {\n        for i in 0..2 {\n            step(i);\n        }\n    }\n}\n";
    let spans = extract_loop_spans("f.rs", src);
    assert_eq!(spans, vec![(4, 6)]);
}

#[test]
fn extract_loop_spans_single_line_loop_has_equal_start_end() {
    let src = "fn f() {\n    while cond() { step(); }\n}\n";
    let spans = extract_loop_spans("f.rs", src);
    assert_eq!(spans, vec![(2, 2)]);
}

#[test]
fn extract_loop_spans_no_loop_yields_empty() {
    let src = "fn f(x: u32) -> u32 {\n    x + 1\n}\n";
    assert!(extract_loop_spans("f.rs", src).is_empty());
}

#[test]
fn extract_loop_spans_parse_failure_yields_empty() {
    assert!(extract_loop_spans("bad.rs", "fn f(:\n").is_empty());
}
