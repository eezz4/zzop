// The second of the two backends implementing the SAME three routes — see be-catalog-a/provides.ts for
// why the pair exists. Byte-identical route set on purpose: it is the collision that makes every
// `fe-catalog` call ambiguous, and ambiguity is what makes the join produce no edge at all.
declare const apiRoutes: { get(p: string, h: unknown): void; post(p: string, h: unknown): void };
declare const handler: unknown;

apiRoutes.get('/catalog/items', handler);
apiRoutes.get('/catalog/status', handler);
apiRoutes.post('/catalog/orders', handler);

export const marker = 1;
