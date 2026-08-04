use super::*;

// One fixture per span source the module doc names (statement loops, eager comprehensions), plus the
// lazy-silence pins (generator expressions), the `else:`-exclusion pin, a nested-loop ordering pin and
// a no-loop empty-result pin — mirrors `zzop_parser_go::lang::loop_spans`'s test shape.

#[test]
fn extract_loop_spans_for_and_while_include_header() {
    let src = "def f():\n    for i in range(10):\n        do_thing()\n    while cond():\n        step()\n";
    let spans = extract_loop_spans("f.py", src);
    assert_eq!(spans, vec![(2, 3), (4, 5)]);
}

#[test]
fn extract_loop_spans_async_for() {
    let src = "async def f():\n    async for x in gen():\n        use(x)\n";
    let spans = extract_loop_spans("f.py", src);
    assert_eq!(spans, vec![(2, 3)]);
}

/// The `else:` block runs at most ONCE (after normal completion) — the span must stop at the loop
/// BODY's last statement, or a one-shot `finish()` would be claimed per-iteration (module doc).
#[test]
fn extract_loop_spans_for_else_excludes_the_else_block() {
    let src = "def f():\n    for x in xs:\n        use(x)\n    else:\n        finish()\n";
    let spans = extract_loop_spans("f.py", src);
    assert_eq!(spans, vec![(2, 3)]);
}

#[test]
fn extract_loop_spans_while_else_excludes_the_else_block() {
    let src = "def f():\n    while cond():\n        step()\n    else:\n        finish()\n";
    let spans = extract_loop_spans("f.py", src);
    assert_eq!(spans, vec![(2, 3)]);
}

#[test]
fn extract_loop_spans_nested_loops_emit_outer_first() {
    let src = "def f():\n    for i in a:\n        for j in b:\n            use(i, j)\n";
    let spans = extract_loop_spans("f.py", src);
    assert_eq!(spans, vec![(2, 4), (3, 4)]);
}

#[test]
fn extract_loop_spans_eager_multiline_comprehensions_are_spans() {
    let src = "def f(xs):\n    a = [g(x)\n         for x in xs]\n    b = {g(x)\n         for x in xs}\n    c = {x: g(x)\n         for x in xs}\n";
    let spans = extract_loop_spans("f.py", src);
    assert_eq!(spans, vec![(2, 3), (4, 5), (6, 7)]);
}

/// A SINGLE-LINE comprehension is deliberately NOT a span (module doc): its `(n, n)` span cannot be
/// told apart, line-granularly, from the one-shot calls sharing the line. The review-reproduced shape:
/// the `print` here runs ONCE, but it sits on the comprehension's only line — a `(2, 2)` span made
/// `console-in-loop` fire on it. Intended under-reporting: a per-iteration call INSIDE a one-line
/// comprehension is lost too, pinned by the second fixture.
#[test]
fn extract_loop_spans_single_line_comprehension_is_not_emitted() {
    let src = "def report(rows):\n    print(\"ids:\", [r.id for r in rows])\n";
    assert!(extract_loop_spans("f.py", src).is_empty());
    let src = "def f(xs):\n    return [g(x) for x in xs]\n";
    assert!(extract_loop_spans("f.py", src).is_empty());
}

/// The lazy-silence pin (module doc's contract boundary): a generator expression builds a generator
/// object and runs its element code ZERO times unless consumed — never a span.
#[test]
fn extract_loop_spans_generator_expression_is_silent() {
    let src = "def f(xs):\n    g = (fetch(x)\n         for x in xs)\n    return g\n";
    assert!(extract_loop_spans("f.py", src).is_empty());
}

/// The walk still descends into a genexp: an EAGER comprehension nested inside one is recorded on its
/// own terms, while the enclosing genexp itself stays silent.
#[test]
fn extract_loop_spans_eager_comp_nested_in_genexp_is_recorded_genexp_is_not() {
    let src = "def f(rows):\n    g = (sum([h(x)\n              for x in row])\n         for row in rows)\n    return g\n";
    let spans = extract_loop_spans("f.py", src);
    assert_eq!(spans, vec![(2, 3)]);
}

#[test]
fn extract_loop_spans_module_level_loop_is_recorded() {
    let src = "for x in xs:\n    use(x)\n";
    let spans = extract_loop_spans("f.py", src);
    assert_eq!(spans, vec![(1, 2)]);
}

/// STATEMENT loops keep their one-line spans — the single-line skip is comprehension-arm-only
/// (module doc: the residual `stmt; for` line-share ambiguity is published, not fixed).
#[test]
fn extract_loop_spans_single_line_loop_has_equal_start_end() {
    let src = "def f():\n    for x in xs: use(x)\n";
    let spans = extract_loop_spans("f.py", src);
    assert_eq!(spans, vec![(2, 2)]);
}

#[test]
fn extract_loop_spans_no_loop_yields_empty() {
    let src = "def f(x):\n    return x + 1\n";
    assert!(extract_loop_spans("f.py", src).is_empty());
}

#[test]
fn extract_loop_spans_parse_failure_yields_empty() {
    assert!(extract_loop_spans("bad.py", "def f(:\n").is_empty());
}
