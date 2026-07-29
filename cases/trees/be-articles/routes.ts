// Cross-layer PROVIDER for the fe-axios pair. These four routes are what that front end actually calls;
// it just writes its client-side paths without the `/api` its axios instance prepends at runtime.
//
// They are reached with full literal paths by trees/fe-axios/server/loader.ts, so they JOIN and none of
// them reports `cross-layer/unconsumed-endpoint` — see that file for why the fixture is built that way
// (the product's 20-key bucketKeys cap, which this corpus is one dead route away from).
declare const apiRoutes: { get(p: string, h: unknown): void; post(p: string, h: unknown): void };
declare const handler: unknown;

apiRoutes.get('/api/articles', handler); // cache: 60s
apiRoutes.get('/api/tags', handler); // cache: 60s
apiRoutes.get('/api/authors', handler); // cache: 60s

// Read-only on purpose. trees/fe-axios POSTs to this route's effective path trying to create a draft,
// which is the defect `cross-layer/method-mismatch` reports at that consume.
apiRoutes.get('/api/articles/drafts', handler); // cache: 60s

export const marker = 1;
