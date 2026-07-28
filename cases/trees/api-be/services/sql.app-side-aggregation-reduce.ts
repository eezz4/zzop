// sql/app-side-aggregation-reduce — bad: reduce a findMany() result in app code. good: aggregate in the DB.
type Model = {
  findMany: (a?: unknown) => Promise<{ amount: number }[]>;
  aggregate: (a: unknown) => Promise<unknown>;
};
declare const prisma: { order: Model };

export async function bad() {
  const rows = await prisma.order.findMany();
  return rows.reduce((sum, r) => sum + r.amount, 0);
}

export function good() {
  return prisma.order.aggregate({ _sum: { amount: true } });
}
