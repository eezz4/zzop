// Second cross-layer PROVIDER tree — its provides intentionally collide with xlayer-be's to drive
// cross-layer/{duplicate-route, ambiguous-consume, route-shadowing}. Distinct source (`xbe2`) so the
// collisions span trees (single-tree duplicate-route/route-shadowing require same-file/same-tree instead).
declare const apiRoutes: { get(p: string, h: unknown): void; post(p: string, h: unknown): void };
declare const handler: unknown;

// SAME key as xlayer-be's `GET /widgets` — 2 distinct sources provide it → cross-layer/duplicate-route,
// and since xlayer-fe consumes `GET /widgets` it is also cross-layer/ambiguous-consume (which provider answers?).
apiRoutes.get('/widgets', handler);

// Pattern route `GET /users/{}` — would shadow xlayer-be's LITERAL `GET /users/export` behind a shared
// first-match gateway → cross-layer/route-shadowing (anchored here, at the pattern's provide site).
apiRoutes.get('/users/:id', handler);

// Pattern route `GET /items/{}` — xlayer-fe's unprovided consume `GET /items/detail` matches after
// allowing the `{}` position to differ → cross-layer/path-near-miss.
apiRoutes.get('/items/:id', handler);

export const marker = 1;
