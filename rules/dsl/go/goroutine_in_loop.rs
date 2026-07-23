//! `go-goroutine-in-loop` tests (split from `go.rs`, mirroring `rules/dsl/be-db/client_lifecycle.rs`'s
//! own per-rule split).

use super::*;

#[test]
fn goroutine_started_inside_a_range_loop_is_flagged() {
    let dir = TempDir::new("zzop-go");
    dir.write(
        "worker.go",
        "package main\n\nfunc f(items []int) {\n\tfor _, it := range items {\n\t\tgo process(it)\n\t}\n}\n\nfunc process(it int) {}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "go-goroutine-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

/// Negative pin: a single `go worker()` OUTSIDE any loop must not fire — proves the containment gate is
/// real (structural, via projected loop spans), not mere co-occurrence of `go` and `for` anywhere in the
/// file/function.
#[test]
fn goroutine_outside_any_loop_is_not_flagged() {
    let dir = TempDir::new("zzop-go");
    dir.write(
        "worker.go",
        "package main\n\nfunc f() {\n\tgo worker()\n}\n\nfunc worker() {}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "go-goroutine-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

/// Negative pin: a loop that calls the same work synchronously (no `go`) must not fire — the pattern
/// itself (not just the loop containment) has to match.
#[test]
fn synchronous_call_inside_loop_is_not_flagged() {
    let dir = TempDir::new("zzop-go");
    dir.write(
        "worker.go",
        "package main\n\nfunc f(items []int) {\n\tfor _, it := range items {\n\t\tprocess(it)\n\t}\n}\n\nfunc process(it int) {}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "go-goroutine-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn goroutine_in_loop_ok_marker_directly_above_the_go_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-go");
    dir.write(
        "worker.go",
        "package main\n\nfunc f(items []int) {\n\tfor _, it := range items {\n\t\t// goroutine-in-loop-ok: bounded fixture list, single-shot job runner\n\t\tgo process(it)\n\t}\n}\n\nfunc process(it int) {}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "go-goroutine-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

/// Regression pin, same defect class as `zzop_parser_go::lang::symbols`'s leading-comment
/// `body_line_range` bug: the ENCLOSING FUNCTION's body opens with a standalone `//` comment before the
/// loop even starts. Before the fix, that comment (an "extra" tree-sitter splices in as a named child of
/// `block`) stole the position `body_line_range` assumed belonged to `statement_list`, so the whole
/// function projected `body_start: None, body_end: None` — and `MethodScan` (which this rule is built
/// on, see `crates::core::dsl::method_scan`) skips any symbol with no body span entirely, so this rule
/// silently never fired for ANY Go function opening with a comment, loop or no loop. Proves the shipped
/// rule now fires where it used to silently miss.
#[test]
fn goroutine_in_loop_fires_when_enclosing_function_body_opens_with_a_comment() {
    let dir = TempDir::new("zzop-go");
    dir.write(
        "worker.go",
        "package main\n\nfunc f(items []int) {\n\t// dispatches one worker per item\n\tfor _, it := range items {\n\t\tgo process(it)\n\t}\n}\n\nfunc process(it int) {}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "go-goroutine-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

/// `go func(){...}()` (anonymous closure form) is also recognized, not just a named-function call.
#[test]
fn goroutine_anonymous_closure_inside_loop_is_flagged() {
    let dir = TempDir::new("zzop-go");
    dir.write(
        "worker.go",
        "package main\n\nfunc f(items []int) {\n\tfor _, it := range items {\n\t\tgo func() {\n\t\t\tprocess(it)\n\t\t}()\n\t}\n}\n\nfunc process(it int) {}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "go-goroutine-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}
