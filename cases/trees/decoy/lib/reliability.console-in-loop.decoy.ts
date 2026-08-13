// DECOY for code-hygiene/console-in-loop. In scope, provably: the rule's file_pattern admits any `.ts`
// with no path gate, and this file's `console.log` calls ARE projected `console-write` call sites —
// verified by mutating one into the loop body, confirming the rule fires, and restoring. Only the loop
// containment is missing, which is the whole point.
//
// `lib/` and not `api/` on purpose: code-hygiene/console-in-be gates on a backend path segment, so a
// console write under `decoy/api/` would fire THAT rule and this file could never be a clean control.
// The console-in-be boundary has its own decoy at ../api/reliability.console-in-be.decoy.ts.
//
// Four probes of the rule's stated boundary, all of which a co-occurrence matcher would fail:
//   1. a console write in a function that ALSO contains a loop, but outside it — the exact 11/11
//      false-positive shape a field audit found in perf/api-in-loop before loop spans replaced token
//      co-occurrence, and the reason this rule exists on the call-site channel at all;
//   2. a console call named only in a comment and in a string literal INSIDE a loop body — never a site,
//      so the containment question is never asked;
//   3. a structured logger called inside a loop — configured output with levels and sinks is not a
//      console write, and folding it in would be false;
//   4. a ONE-SHOT console write sharing its single line with an eager `.map` callback — the callback
//      span would be `(n, n)`, which a line-granular channel cannot separate from the one-shot calls
//      around it, so the producer emits no span there at all (`SourceFile::loop_spans`'s single-line
//      rule; the shape a review reproduced firing before that rule existed).
declare const logger: { debug(message: string): void };

export function summarize(rows: string[]): number {
  let total = 0;
  for (const row of rows) {
    total += row.length;
  }
  console.log('summarized ' + rows.length + ' rows');
  return total;
}

export function annotate(rows: string[]): string[] {
  return rows.map((row) => {
    // console.log(row)
    logger.debug('use console.log(row) here when debugging locally');
    return row.trim();
  });
}

export function joinIds(items: Array<{ id: string }>): void {
  console.log(items.map((item) => item.id).join(','));
}
