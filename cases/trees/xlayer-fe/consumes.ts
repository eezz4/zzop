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
