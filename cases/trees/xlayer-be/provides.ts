// Cross-layer PROVIDER tree — registers HTTP routes that the xlayer-fe tree consumes (or fails to),
// driving cross-layer/{method-mismatch, version-skew, unprovided-mutation-call, ...}.
declare const apiRoutes: { get(p: string, h: unknown): void; post(p: string, h: unknown): void };
declare const handler: unknown;

apiRoutes.get('/widgets', handler);
apiRoutes.get('/v1/gadgets', handler);
apiRoutes.post('/orders', handler);

// LITERAL route shadowed by xlayer-be2's pattern `GET /users/{}` across trees → cross-layer/route-shadowing
// (finding anchors at the pattern in xlayer-be2, not here).
apiRoutes.get('/users/export', handler);

// Also calls vendor.example.com — same host is consumed by xlayer-fe too, from a DISTINCT source tree:
// cross-layer/external-duplicated-integration.
declare function fetch(u: string): Promise<unknown>;
export function callVendor() {
  return fetch('https://vendor.example.com/status');
}

export const marker = 1;
