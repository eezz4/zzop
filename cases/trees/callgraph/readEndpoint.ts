// unsafe-read-endpoint — a GET/HEAD ("safe" per RFC 7231) route whose handler reaches a store WRITE via
// call-graph BFS. The write is a store-shaped call (`db.create(...)`, receiver `db` matches the ORM
// receiver vocabulary `^db$|^prisma$|Repository$|Store$|...`) inside the handler's own body (depth 0).
declare const apiRoutes: { get(p: string, h: unknown): void };
declare const db: { create(x: unknown): Promise<void> };

export async function statsWithWrite() {
  await db.create({ hit: 1 }); // WRITE on a GET path → crawler/prefetch/retry hazard
  return { ok: true };
}

apiRoutes.get('/stats', statsWithWrite);
