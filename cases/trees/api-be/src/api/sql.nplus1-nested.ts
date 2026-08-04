// sql/nplus1 at a NESTED `api/` segment — the shape the rule was structurally blind to until
// 2026-08-01, when its file_pattern was root-anchored and matched only a tree-ROOT `api/` directory.
// The sibling at `api/api/sql.nplus1.ts` covers the root-level path; this file is what makes that
// alignment regression-visible, because until it existed the fix's detection-gate delta was zero and
// nothing would have gone red if the anchor came back. Same body as the sibling, deliberately: the only
// variable under test here is the PATH.
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
