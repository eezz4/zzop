// be-db/update-delete-no-where — bad: deleteMany with no where (whole-table wipe). good: a scoped where.
type Order = { deleteMany: (a?: unknown) => Promise<unknown> };
declare const prisma: { order: Order };

export function bad() {
  return prisma.order.deleteMany();
}

export function good(customerId: string) {
  return prisma.order.deleteMany({ where: { customerId } });
}
