// sql/nplus1 at a nested `domains/<name>/routes/` segment — the second arm of the same path scope, and
// the one the corpus had NO instance of at any depth. Its sibling `src/api/sql.nplus1-nested.ts` covers
// the `api/` arm. Same body as both, so the only variable under test is the PATH.
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
