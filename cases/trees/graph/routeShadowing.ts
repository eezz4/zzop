// route-shadowing (single-tree) — a param-segment route registered BEFORE a same-shape literal route of
// the same method makes the literal route unreachable in a first-match router. Uses `apiRoutes` (the
// default router-mount identifier the recognizer keys on; a bare `router.get` is NOT recognized) and
// `/accounts/*` paths kept distinct from the cross-tree `/users/*` shadowing pair (xlayer-be/xlayer-be2)
// so the two scopes never entangle.
declare const apiRoutes: { get(path: string, handler: unknown): void };
declare const byId: unknown;
declare const me: unknown;

apiRoutes.get('/accounts/:id', byId); // param route registered first — shadows the literal below
apiRoutes.get('/accounts/me', me); // literal, unreachable behind the param route in a first-match router

export const marker = 1;
