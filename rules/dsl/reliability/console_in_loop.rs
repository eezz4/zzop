use crate::{hits, scan, TempDir};

// --- console-in-loop ---
//
// The rule this channel exists to make expressible. `console-in-be` asks WHERE a console write is;
// this one asks whether the parser PROVED it runs once per iteration, which is a containment question
// no text matcher can answer — the same question `perf/api-in-loop` was rebuilt around after a field
// audit found its loop-token co-occurrence version 11/11 false positives.
//
// Two facts must both be present for a finding: a projected call site (`call_sites`, the TypeScript and
// Python producers) and a projected loop span (`loop_spans`). Either one absent = silence, never a
// permissive fallback, so every negative below has to distinguish "the rule correctly declined" from
// "the rule could not see" — which is why the positives are asserted first in each pair.

/// Asserts the rule fires exactly once in `src`, on `line`.
fn fires(path: &str, src: &str, line: u32) {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(path, src);
    let out = scan(&dir);
    let h = hits(&out, "console-in-loop");
    assert_eq!(h.len(), 1, "{path}: {:?}", out.findings);
    assert_eq!(h[0].line, line, "{path}: {:?}", out.findings);
}

/// Asserts the rule is silent on `src`.
fn silent(path: &str, src: &str) {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(path, src);
    let out = scan(&dir);
    let h = hits(&out, "console-in-loop");
    assert!(h.is_empty(), "{path}: {:?}", out.findings);
}

// --- the structural claim, both directions ---

#[test]
fn a_console_write_inside_a_for_statement_fires() {
    fires(
        "src/report.ts",
        "export function report(rows: string[]) {\n  for (const row of rows) {\n    console.log(row);\n  }\n}\n",
        3,
    );
}

#[test]
fn the_same_console_write_outside_the_loop_is_silent() {
    // THE pair that makes the previous test mean something. Same file shape, same call, same function —
    // only the containment differs. A rule that merely co-located a console write with loop syntax would
    // fire here, and that is exactly the defect class this channel was built to retire.
    silent(
        "src/report.ts",
        "export function report(rows: string[]) {\n  for (const row of rows) {\n    accumulate(row);\n  }\n  console.log(`done: ${rows.length}`);\n}\n",
    );
}

#[test]
fn a_console_write_inside_an_array_iteration_callback_fires() {
    // `loop_spans` projects the eager Array callbacks as spans, so a `forEach` body is inside a loop by
    // the same fact a `for` body is — the boundary is `SourceFile::loop_spans`' to own, and this pins
    // that this rule inherits it rather than re-deciding it.
    fires(
        "src/report.ts",
        "export function report(rows: string[]) {\n  rows.forEach((row) => {\n    console.error(row);\n  });\n}\n",
        3,
    );
}

// --- the level axis: WIDER than console-in-be, on purpose ---

#[test]
fn debug_and_trace_count_here_even_though_console_in_be_excludes_them() {
    // The one deliberate divergence between the two console rules, pinned so it cannot be "tidied" into
    // agreement. `console-in-be` names four methods because its concern is structured-logging hygiene;
    // this rule names none, because its concern is per-iteration volume and blocking, which a level
    // cannot change. Both rules read the same six-method producer set.
    fires(
        "src/report.ts",
        "export function report(rows: string[]) {\n  for (const row of rows) {\n    console.debug(row);\n  }\n}\n",
        3,
    );
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/report.ts",
        "export function report(rows: string[]) {\n  for (const row of rows) {\n    console.trace(row);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "console-in-loop").len(), 1, "{:?}", out.findings);
    assert!(
        hits(&out, "console-in-be").is_empty(),
        "console-in-be must stay out of the debug/trace lane: {:?}",
        out.findings
    );
}

// --- the parse dividend: what a text matcher would have got wrong ---

#[test]
fn a_console_call_named_only_in_a_string_or_a_comment_is_not_a_site() {
    // Not a `skip_comment_lines` flag doing this — there is no such flag on this matcher. The call
    // simply never becomes a site, so the containment question is never even asked.
    silent(
        "src/report.ts",
        "export function report(rows: string[]) {\n  for (const row of rows) {\n    // console.log(row)\n    emit(\"console.log(row)\", row);\n  }\n}\n",
    );
}

#[test]
fn a_structured_logger_call_in_a_loop_is_not_a_console_write() {
    // The false fold the channel refuses at the PRODUCER, asserted from the rule side so a later widening
    // of `console-write` cannot land without this turning red. A rule banning console writes in a loop is
    // not a rule banning logging in a loop.
    silent(
        "src/report.ts",
        "declare const logger: { info(m: string): void };\nexport function report(rows: string[]) {\n  for (const row of rows) {\n    logger.info(row);\n  }\n}\n",
    );
}

// --- Python: the same rule, no second copy ---

#[test]
fn a_python_print_inside_a_for_statement_fires() {
    // The whole point of the channel: one rule, two languages, no per-language regex copy. `print` is a
    // `console-write` with the callee spelled as Python spells it.
    fires(
        "src/report.py",
        "def report(rows):\n    for row in rows:\n        print(row)\n",
        3,
    );
}

#[test]
fn a_python_print_outside_the_loop_is_silent() {
    silent(
        "src/report.py",
        "def report(rows):\n    for row in rows:\n        accumulate(row)\n    print(len(rows))\n",
    );
}

#[test]
fn a_python_print_inside_a_multiline_comprehension_fires() {
    // Comprehensions are IN Python's `loop_spans` (generator expressions are not) — a boundary
    // `SourceFile::loop_spans` owns and this rule inherits. Pinned here rather than restated, so a change
    // to that boundary surfaces as a rule-level behavior change instead of silently moving. MULTILINE on
    // purpose: single-line comprehension spans are not emitted at all (the pair below).
    fires(
        "src/report.py",
        "def report(rows):\n    return [print(row)\n            for row in rows]\n",
        2,
    );
}

// --- the single-line boundary: a (n, n) callback/comprehension span proves nothing ---

#[test]
fn a_one_shot_console_write_sharing_a_one_line_map_callbacks_line_is_silent() {
    // Review-reproduced false positive, planted exactly as reproduced: `.map((i) => i.id)`'s callback
    // span was `(2, 2)`, the same line the ONE-SHOT `console.log` and `.join` sit on, so line-granular
    // containment swept them "into the loop". The producer now drops single-line callback spans
    // (`SourceFile::loop_spans`'s doc owns the rule) — silence here is the fix, and the multiline
    // `forEach` positive above is the pair proving real per-iteration writes still fire.
    silent(
        "src/report.ts",
        "export function report(items: { id: string }[]) {\n  console.log(items.map((i) => i.id).join(','));\n}\n",
    );
}

#[test]
fn a_one_shot_python_print_sharing_a_one_line_comprehensions_line_is_silent() {
    // The Python twin: the comprehension span was `(2, 2)` and the one-shot `print` shares that line.
    // Same producer rule, same intended under-reporting cost (a per-iteration print INSIDE a one-line
    // comprehension is also lost); the multiline comprehension positive above is the pair.
    silent(
        "src/report.py",
        "def report(rows):\n    print(\"ids:\", [r.id for r in rows])\n",
    );
}

// --- suppression ---

#[test]
fn the_ok_marker_suppresses_in_both_comment_syntaxes() {
    // `//` and `#` both, because this matcher is multi-language from its first wave and there is no
    // per-language rule copy whose `file_pattern` would have narrowed the comment leader.
    silent(
        "src/report.ts",
        "export function report(rows: string[]) {\n  for (const row of rows) {\n    // zzop-console-in-loop-ok: dev-only trace, gated by NODE_ENV upstream\n    console.log(row);\n  }\n}\n",
    );
    silent(
        "src/report.py",
        "def report(rows):\n    for row in rows:\n        # zzop-console-in-loop-ok: dev-only trace, gated upstream\n        print(row)\n",
    );
}
