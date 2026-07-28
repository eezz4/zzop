// sql/count-in-loop (file_pattern ^api/) — bad: a count() per loop iteration. good: a single grouped count.
type Model = { count: (a?: unknown) => Promise<number>; groupBy: (a: unknown) => Promise<unknown[]> };
declare const prisma: { order: Model };

export async function bad(userIds: string[]) {
  const out: number[] = [];
  for (const id of userIds) {
    out.push(await prisma.order.count({ where: { id } }));
  }
  return out;
}

export function good() {
  return prisma.order.groupBy({ by: ['userId'], _count: true });
}
