// Cross-layer CONSUMER tree — fetches that join (or mismatch) xlayer-be's provides, plus external calls.
declare function fetch(u: string, init?: unknown): Promise<unknown>;

export function calls() {
  fetch('/widgets', { method: 'post' }); // method-mismatch: xlayer-be provides GET /widgets
  fetch('/v2/gadgets'); // version-skew: xlayer-be provides GET /v1/gadgets
  fetch('http://10.0.0.5/health'); // external-ip-literal
  fetch('https://vendor.example.com/api?token=secret123'); // external-secret-in-url
  fetch('/missing', { method: 'delete' }); // unprovided-mutation-call: DELETE with no provider
  fetch('https://prod.internal.example.com/widgets'); // external-shadow-internal: hardcoded host reaches internal GET /widgets (xlayer-be provides it)
  fetch('/widgets'); // ambiguous-consume: GET /widgets is provided by BOTH xlayer-be and xlayer-be2
  fetch('/items/detail'); // path-near-miss: unprovided GET /items/detail matches provide GET /items/{} (xlayer-be2)
}

export function externalDrift() {
  fetch('https://a.example.com/report/daily'); // external-base-url-drift (same 2-seg path, sibling host + shared registrable domain below)
  fetch('https://b.example.com/report/daily');
  fetch('https://gw.example.com/v1/pay'); // external-version-inconsistent (v1 + versionless below)
  fetch('https://gw.example.com/charge');
}

// APPENDED AT THE END ON PURPOSE — EXPECTED.jsonc keys are `file:line`, so an insertion above would
// re-anchor every existing expectation in this file.
//
// cross-layer/retrying-write-no-idempotency, as a PAIR that differs in exactly one factor. `pRetry` is a
// distinctive retry-wrapper identifier the egress recognizer knows by name alone (no import needed), so
// the call it encloses carries `IoConsume::retry_configured`; POST /charges is really provided by
// xlayer-be and carries no witnessed `idempotency-guarded` attribute, so both sides of the rule's
// two-sided check are real rather than assumed-absent.
declare function pRetry<T>(fn: () => Promise<T>): Promise<T>;

export function retriedWrite() {
  pRetry(() => fetch('/charges', { method: 'post' })); // retrying-write-no-idempotency
}

// SILENCE CONTROL for the line above, one factor different: the SAME joined write, not wrapped in a
// retry. The rule's trigger is the retry tag, not the write, so this line must stay silent for it — and
// unlike a decoy file this control is in scope beyond doubt, because its twin four lines up fires.
export function plainWrite() {
  fetch('/charges', { method: 'post' });
}
