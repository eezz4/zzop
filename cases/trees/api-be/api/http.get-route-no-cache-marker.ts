// http/get-route-no-cache-marker — bad: apiRoutes.get with no cache-strategy marker. good: a `// cache:`
// marker on the same line documents the read-model caching decision.
declare const apiRoutes: { get(path: string, handler: unknown): void };
declare const listUsers: unknown;

export function bad() {
  apiRoutes.get('/read-models', listUsers); // unique path: /users began being consumed by gateway.ts's keyed template
}

export function good() {
  apiRoutes.get('/accounts', listUsers); // cache: 60s
}
