// sql/nplus1, root-level `api/` — bad: await a store call per loop iteration. good: one batched query.
type Order = { findMany: (a: unknown) => Promise<unknown[]> };
declare const prisma: { order: Order };

export async function bad(ids: string[]) {
  const out: unknown[] = [];
  for (const id of ids) {
    out.push(await prisma.order.findMany({ where: { id } }));
  }
  return out;
}

export async function good(ids: string[]) {
  return prisma.order.findMany({ where: { id: { in: ids } } });
}
