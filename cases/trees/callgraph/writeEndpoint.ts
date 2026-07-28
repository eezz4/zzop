// non-idempotent-write — a PUT/DELETE route (idempotency-promising) whose handler reaches a `create`
// (a retry duplicates the row). PUT also has no auth guard in its call-graph, so mutating-route-no-auth
// legitimately co-fires here (two independent defects on one route).
declare const apiRoutes: { put(p: string, h: unknown): void };
declare const db: { create(x: unknown): Promise<void> };

export async function upsertRecord() {
  await db.create({ v: 1 }); // create() under PUT — non-idempotent (a retried PUT should not duplicate)
}

apiRoutes.put('/records/:id', upsertRecord);
