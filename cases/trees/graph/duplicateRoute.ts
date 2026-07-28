// duplicate-route — the same (METHOD, path) registered twice in the tree.
declare const apiRoutes: { get(path: string, handler: unknown): void };
declare const listV1: unknown;
declare const listV2: unknown;

apiRoutes.get('/reports', listV1); // cache: n/a
apiRoutes.get('/reports', listV2); // cache: n/a

export const marker = 1;
