use super::*;

// One fixture per C# loop form the module doc names, plus the LINQ-lambda lazy-silence pin, a
// nested-loop ordering pin and a no-loop empty-result pin — mirrors `zzop_parser_go`/
// `zzop_parser_java_21::lang::loop_spans`'s test shape.

#[test]
fn extract_loop_spans_classic_for_include_header() {
    let src = "class C {\n  void M() {\n    for (int i = 0; i < 10; i++) {\n      DoThing();\n    }\n  }\n}\n";
    let spans = extract_loop_spans("C.cs", src);
    assert_eq!(spans, vec![(3, 5)]);
}

#[test]
fn extract_loop_spans_foreach() {
    let src = "class C {\n  void M(int[] xs) {\n    foreach (var x in xs) {\n      Use(x);\n    }\n  }\n}\n";
    let spans = extract_loop_spans("C.cs", src);
    assert_eq!(spans, vec![(3, 5)]);
}

/// `await foreach` is the same `foreach_statement` kind (the `await` token is a plain child).
#[test]
fn extract_loop_spans_await_foreach() {
    let src = "class C {\n  async Task M(IAsyncEnumerable<int> xs) {\n    await foreach (var x in xs) {\n      Use(x);\n    }\n  }\n}\n";
    let spans = extract_loop_spans("C.cs", src);
    assert_eq!(spans, vec![(3, 5)]);
}

#[test]
fn extract_loop_spans_while() {
    let src = "class C {\n  void M() {\n    while (Cond()) {\n      Step();\n    }\n  }\n}\n";
    let spans = extract_loop_spans("C.cs", src);
    assert_eq!(spans, vec![(3, 5)]);
}

/// The `do` node's span runs through its trailing `while (cond);` line — the condition re-evaluates
/// per iteration, so header-inclusive symmetry holds at the tail here.
#[test]
fn extract_loop_spans_do_while_includes_trailing_condition_line() {
    let src = "class C {\n  void M() {\n    do {\n      Step();\n    } while (Cond());\n  }\n}\n";
    let spans = extract_loop_spans("C.cs", src);
    assert_eq!(spans, vec![(3, 5)]);
}

#[test]
fn extract_loop_spans_nested_loops_emit_outer_first() {
    let src = "class C {\n  void M() {\n    for (int i = 0; i < 2; i++) {\n      for (int j = 0; j < 2; j++) {\n        Use(i, j);\n      }\n    }\n  }\n}\n";
    let spans = extract_loop_spans("C.cs", src);
    assert_eq!(spans, vec![(3, 7), (4, 6)]);
}

/// The lazy-silence pin (module doc's contract boundary): a LINQ operator's lambda runs zero times
/// unless the query is enumerated (deferred execution) — never a span.
#[test]
fn extract_loop_spans_linq_select_lambda_is_silent() {
    let src = "class C {\n  object M(System.Collections.Generic.List<int> xs) {\n    return xs\n        .Select(x => Fetch(x))\n        .ToList();\n  }\n}\n";
    assert!(extract_loop_spans("C.cs", src).is_empty());
}

#[test]
fn extract_loop_spans_single_line_loop_has_equal_start_end() {
    let src = "class C {\n  void M() {\n    while (Cond()) { Step(); }\n  }\n}\n";
    let spans = extract_loop_spans("C.cs", src);
    assert_eq!(spans, vec![(3, 3)]);
}

#[test]
fn extract_loop_spans_no_loop_yields_empty() {
    let src = "class C {\n  int M() {\n    return 1;\n  }\n}\n";
    assert!(extract_loop_spans("C.cs", src).is_empty());
}

#[test]
fn extract_loop_spans_hopeless_parse_yields_empty() {
    assert!(extract_loop_spans("bad.cs", "\u{0}\u{1}not csharp at all{{{{").is_empty());
}
