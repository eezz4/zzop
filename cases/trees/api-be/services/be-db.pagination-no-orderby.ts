// be-db/pagination-no-orderby — bad: skip/take with no orderBy (unstable page boundaries). good: a stable sort.
type Model = { findMany: (a: unknown) => Promise<unknown[]> };
declare const prisma: { order: Model };

export function bad(skip: number, take: number) {
  return prisma.order.findMany({ skip, take });
}

export function good(skip: number, take: number) {
  return prisma.order.findMany({ skip, take, orderBy: { id: 'asc' } });
}
