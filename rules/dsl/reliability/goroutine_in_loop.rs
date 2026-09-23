//! `goroutine-in-loop` tests (split from `go.rs`, mirroring `rules/dsl/db/client_lifecycle.rs`'s
//! own per-rule split).
//!
//! Arrived from `rules/dsl/go/` on 2026-09-03 (see the pack root's module doc for why). The rule runs
//! against real `zzop_parser_go`-derived loop spans, not hand-built ones, and its `suppress_marker` is
//! exercised once below with the marker directly above the reported line (`MARKER_LOOKBACK_LINES` = 1 —
//! the only lookback distance that suppresses).

use super::*;

#[test]
fn goroutine_started_inside_a_range_loop_is_flagged() {
    let dir = TempDir::new("zzop-go");
    dir.write(
        "worker.go",
        "package main\n\nfunc f(items []int) {\n\tfor _, it := range items {\n\t\tgo process(it)\n\t}\n}\n\nfunc process(it int) {}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "goroutine-in-loop");
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
        hits(&out, "goroutine-in-loop").is_empty(),
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
        hits(&out, "goroutine-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn goroutine_in_loop_ok_marker_directly_above_the_go_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-go");
    dir.write(
        "worker.go",
        "package main\n\nfunc f(items []int) {\n\tfor _, it := range items {\n\t\t// zzop-goroutine-in-loop-ok: bounded fixture list, single-shot job runner\n\t\tgo process(it)\n\t}\n}\n\nfunc process(it int) {}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "goroutine-in-loop").is_empty(),
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
    let h = hits(&out, "goroutine-in-loop");
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
    let h = hits(&out, "goroutine-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

/// Recall pin for the shape an auditor reported as a MISS on gogs
/// (`internal/ssh/ssh.go:89`, `d460e50`): the `go` statement sits inside a `switch` arm, inside a
/// `for ... range` over a CHANNEL, inside a function LITERAL that is itself passed to an ordinary
/// call — three nesting levels the containment gate was suspected of losing. It loses none of them:
/// `trigger_in_loop` tests the `go` line against the file's `for_statement` spans, and a channel
/// range and a `select`/`switch` arm are ordinary statements inside one. The finding lands on the
/// `go` line itself.
#[test]
fn goroutine_in_a_switch_arm_inside_a_channel_range_inside_a_closure_is_flagged() {
    let dir = TempDir::new("zzop-go");
    dir.write(
        "worker.go",
        "package main\n\nfunc handle(chans <-chan int, in <-chan int) {\n\tfor range chans {\n\t\trun(func(reqs <-chan int) {\n\t\t\tfor req := range reqs {\n\t\t\t\tswitch req {\n\t\t\t\tcase 1:\n\t\t\t\t\tgo work()\n\t\t\t\tdefault:\n\t\t\t\t}\n\t\t\t}\n\t\t}, in)\n\t}\n}\n\nfunc run(f func(<-chan int), c <-chan int) { f(c) }\n\nfunc work() {}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "goroutine-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 9);
    assert_eq!(h[0].data.as_ref().unwrap()["triggerLines"], 1);
}

/// BOUNDARY PIN — the cardinality this rule actually ships, and the reason the gogs auditor read a
/// containment miss where there was none. `Matcher::MethodScan` emits ONE finding per symbol body
/// span, anchored on the FIRST qualifying trigger line (`dsl/method_scan.rs`'s
/// `trigger_hit.or(...)`); every later qualifying `go` line in the same function is counted in
/// `data.triggerLines` and gets no line of its own. This fixture is the reduced gogs
/// `handleServerConn` shape — two `go` statements, both inside projected loop spans, in one function
/// — and it asserts BOTH halves: one finding on the first `go`, and `triggerLines == 2` proving the
/// second one passed the containment gate rather than being dropped by it.
#[test]
fn two_qualifying_goroutines_in_one_function_collapse_to_one_finding_on_the_first() {
    let dir = TempDir::new("zzop-go");
    dir.write(
        "worker.go",
        "package main\n\nfunc handle(chans <-chan int, in <-chan int) {\n\tfor range chans {\n\t\tgo func(reqs <-chan int) {\n\t\t\tfor req := range reqs {\n\t\t\t\tswitch req {\n\t\t\t\tcase 1:\n\t\t\t\t\tgo func() {\n\t\t\t\t\t\twork()\n\t\t\t\t\t}()\n\t\t\t\tdefault:\n\t\t\t\t}\n\t\t\t}\n\t\t}(in)\n\t}\n}\n\nfunc work() {}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "goroutine-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
    assert_eq!(h[0].data.as_ref().unwrap()["triggerLines"], 2);
}

/// MESSAGE-HONESTY PIN. The cardinality pinned directly above is invisible from a finding's line and
/// snippet alone, so the message has to say it: a reader who sees one finding must not conclude the
/// function holds one unbounded spawn. Asserted on a REAL emitted finding (not on the pack JSON), so
/// the sentence is checked on the text a user actually receives.
#[test]
fn the_message_discloses_that_one_finding_covers_every_qualifying_go_line_in_the_function() {
    let dir = TempDir::new("zzop-go");
    dir.write(
        "worker.go",
        "package main\n\nfunc handle(chans <-chan int, in <-chan int) {\n\tfor range chans {\n\t\tgo func(reqs <-chan int) {\n\t\t\tfor req := range reqs {\n\t\t\t\tswitch req {\n\t\t\t\tcase 1:\n\t\t\t\t\tgo func() {\n\t\t\t\t\t\twork()\n\t\t\t\t\t}()\n\t\t\t\tdefault:\n\t\t\t\t}\n\t\t\t}\n\t\t}(in)\n\t}\n}\n\nfunc work() {}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "goroutine-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let msg = &h[0].message;
    assert!(
        msg.contains("triggerLines"),
        "message must name the field carrying the real per-function count: {msg}"
    );
    assert!(
        msg.contains("ONE FINDING PER ENCLOSING FUNCTION"),
        "message must disclose that later qualifying `go` lines get no finding of their own: {msg}"
    );
}
