// sql/app-side-aggregation-filter-length — bad: count a findMany() result with .filter().length in app
// code. good: count in the DB with a where clause.
type Model = {
  findMany: (a?: unknown) => Promise<{ active: boolean }[]>;
  count: (a: unknown) => Promise<number>;
};
declare const prisma: { order: Model };

export async function bad() {
  const rows = await prisma.order.findMany();
  return rows.filter((r) => r.active).length;
}

export function good() {
  return prisma.order.count({ where: { active: true } });
}
