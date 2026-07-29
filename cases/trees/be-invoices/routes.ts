// Cross-layer PROVIDER for the fe-gensdk pair. Same standing as trees/be-articles/routes.ts: these four
// routes are what that front end actually calls, through a generated client whose descriptor paths carry
// only the suffix. They are reached with full literal paths by trees/fe-gensdk/server/loader.ts, so they
// join and none of them reports `cross-layer/unconsumed-endpoint`.
declare const apiRoutes: { get(p: string, h: unknown): void; post(p: string, h: unknown): void };
declare const handler: unknown;

apiRoutes.get('/api/invoices', handler); // cache: 60s
apiRoutes.get('/api/shipments', handler); // cache: 60s
apiRoutes.get('/api/carriers', handler); // cache: 60s

// Read-only on purpose — trees/fe-gensdk POSTs to this route's effective path.
apiRoutes.get('/api/invoices/exports', handler); // cache: 60s

export const marker = 1;
