// The SERVER half of the same front end (the Next.js-style shape: axios with a `baseURL` in the browser,
// plain `fetch` with full paths during server rendering, because no axios instance is configured there).
//
// Two jobs, both load-bearing:
//
// 1. CONTRAST. These four calls reach the same four be-articles routes as pages/articles.ts, written as
//    full literal paths — and every one of them JOINS. Set beside the axios calls that do not, this puts
//    zzop's capability boundary inside a single tree: it is the spelling of the call, not the endpoint,
//    that decides whether the join happens.
//
// 2. THE PRODUCT'S bucketKeys CAP. `crates/summary/src/output/bucket_keys.rs` caps each cross-layer
//    bucket's distinct-key list at 20 (`DEFAULT_BUCKET_KEYS_LIMIT`), and snapshot.mjs ABORTS the run
//    when a bucket truncates unless it is given `--tolerate-bucket-key-cap`, which detection-gate.sh
//    does not pass. Measured 2026-07-29: this corpus already sits at 19 distinct `unconsumedProvides`
//    keys, so two new dead backend routes are enough to stop the gate producing any score at all. These
//    calls keep the four new routes CONSUMED, which is both the honest state of the fixture (the front
//    end really does call them) and what keeps the corpus under that cap.
declare function fetch(u: string, init?: unknown): Promise<unknown>;

export async function loadArticlesPage() {
  await fetch('/api/articles');
  await fetch('/api/tags');
  await fetch('/api/authors');
  await fetch('/api/articles/drafts');
}
