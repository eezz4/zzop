use super::*;

// One fixture per Java loop form the module doc names, plus the stream-lambda lazy-silence pin, a
// nested-loop ordering pin and a no-loop empty-result pin — mirrors `zzop_parser_go::lang::
// loop_spans`'s test shape.

#[test]
fn extract_loop_spans_classic_for_include_header() {
    let src = "class C {\n  void m() {\n    for (int i = 0; i < 10; i++) {\n      doThing();\n    }\n  }\n}\n";
    let spans = extract_loop_spans("C.java", src);
    assert_eq!(spans, vec![(3, 5)]);
}

#[test]
fn extract_loop_spans_enhanced_for() {
    let src =
        "class C {\n  void m(int[] xs) {\n    for (int x : xs) {\n      use(x);\n    }\n  }\n}\n";
    let spans = extract_loop_spans("C.java", src);
    assert_eq!(spans, vec![(3, 5)]);
}

#[test]
fn extract_loop_spans_while() {
    let src = "class C {\n  void m() {\n    while (cond()) {\n      step();\n    }\n  }\n}\n";
    let spans = extract_loop_spans("C.java", src);
    assert_eq!(spans, vec![(3, 5)]);
}

/// The `do` node's span runs through its trailing `while (cond);` line — the condition re-evaluates
/// per iteration, so header-inclusive symmetry holds at the tail here.
#[test]
fn extract_loop_spans_do_while_includes_trailing_condition_line() {
    let src = "class C {\n  void m() {\n    do {\n      step();\n    } while (cond());\n  }\n}\n";
    let spans = extract_loop_spans("C.java", src);
    assert_eq!(spans, vec![(3, 5)]);
}

#[test]
fn extract_loop_spans_nested_loops_emit_outer_first() {
    let src = "class C {\n  void m() {\n    for (int i = 0; i < 2; i++) {\n      for (int j = 0; j < 2; j++) {\n        use(i, j);\n      }\n    }\n  }\n}\n";
    let spans = extract_loop_spans("C.java", src);
    assert_eq!(spans, vec![(3, 7), (4, 6)]);
}

/// The lazy-silence pin (module doc's contract boundary): a stream pipeline's intermediate-operation
/// lambda runs zero times unless a terminal operation consumes the stream — never a span.
#[test]
fn extract_loop_spans_stream_map_lambda_is_silent() {
    let src = "class C {\n  Object m(java.util.List<Integer> xs) {\n    return xs.stream()\n        .map(x -> fetch(x))\n        .toList();\n  }\n}\n";
    assert!(extract_loop_spans("C.java", src).is_empty());
}

#[test]
fn extract_loop_spans_single_line_loop_has_equal_start_end() {
    let src = "class C {\n  void m() {\n    while (cond()) { step(); }\n  }\n}\n";
    let spans = extract_loop_spans("C.java", src);
    assert_eq!(spans, vec![(3, 3)]);
}

#[test]
fn extract_loop_spans_no_loop_yields_empty() {
    let src = "class C {\n  int m() {\n    return 1;\n  }\n}\n";
    assert!(extract_loop_spans("C.java", src).is_empty());
}

#[test]
fn extract_loop_spans_hopeless_parse_yields_empty() {
    assert!(extract_loop_spans("bad.java", "\u{0}\u{1}not java at all{{{{").is_empty());
}
