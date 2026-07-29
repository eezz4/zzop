// Client-side call sites for the fe-axios pair. Every path is written the way a real call site writes
// it — root relative, with no `/api` — because the prefix lives in services/client.ts. The effective
// runtime route of each call is `/api` + the path below, and trees/be-articles provides exactly those.
//
// Compare server/loader.ts in this same tree: it reaches THE SAME FOUR ROUTES with full literal paths,
// and every one of those joins. Same app, same endpoints, two call styles — one inside zzop's capability
// boundary and one outside it. That contrast is the measurement.
declare const axios: {
  get(url: string): Promise<unknown>;
  post(url: string, body: unknown): Promise<unknown>;
};

// Three same-method reads. They JOIN be-articles, because this tree declares its client's base in
// cases/zzop.config.jsonc (`topology.clientBase: "/api"`) and the engine prepends it to every keyed
// relative http consume here.
//
// Undeclared, they were the DEGRADE path instead: three `prefix`-dimension `cross-layer/route-near-miss`
// findings in one (consume source, provide source, prefix, direction) group — exactly
// `MIN_PREFIX_DRIFT_GROUP` — collapsing into one `cross-layer/prefix-drift` that named the missing
// `/api`. Honest, and visibly not a join. Delete the declaration and that is what comes back.
export function list() {
  return axios.get('/articles');
}

export function tags() {
  return axios.get('/tags');
}

export function authors() {
  return axios.get('/authors');
}

// THE DEFECT THE DECLARATION EXPOSED. be-articles provides `GET /api/articles/drafts` and no POST, so
// this call is a real cross-layer defect and `cross-layer/method-mismatch` fires here. It could not
// before: method-mismatch requires the consume's path to be byte-identical to a provide's, and
// `/articles/drafts` is not `/api/articles/drafts`. The finding was carried as a `gap` entry — labeled
// correct, acknowledged as not-yet-produced — until the base resolved.
//
// `cross-layer/unprovided-mutation-call` fires at this anchor too (a write verb whose target no analyzed
// source provides), before and after, per that rule's own module doc. So the repair was purely additive
// here: a false negative became a true positive and nothing had to be retired at this line.
export function createDraft() {
  return axios.post('/articles/drafts', { title: 'untitled' });
}
