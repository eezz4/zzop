// be-db/idempotency-key-regenerated-in-loop — bad: a fresh idempotency key minted on every pass, so a
// retry of the loop is indistinguishable from a new request and the downstream write happens twice.
// good: one key derived from the item, stable across retries.
//
// GATE: the rule carries `require_file: (?i)idempotenc`, satisfied by both keys below.
//
// The enqueued call is a local queue push rather than a fetch/axios call on purpose: an HTTP call inside
// this loop would also be a `reliability/api-in-loop`, and this is the single-rule fixture for the key.
declare const queue: { push(job: unknown): void };

export function bad(items: readonly string[]): void {
  for (const item of items) {
    queue.push({ item, idempotencyKey: crypto.randomUUID() });
  }
}

// The good half mints the key the SAME way and does it OUTSIDE the loop (review ledger V335). What
// separates the two halves in this rule is `trigger_in_loop` and nothing else, so a control that also
// changed the key's provenance tested the wrong axis: a matcher that stopped checking the loop would
// have reported neither. This one it reports, and must not.
export function good(items: readonly string[]): void {
  const idempotencyKey = crypto.randomUUID();
  for (const item of items) {
    queue.push({ item, idempotencyKey });
  }
}
