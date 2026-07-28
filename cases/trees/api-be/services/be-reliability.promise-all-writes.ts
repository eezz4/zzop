// be-reliability/promise-all-writes — bad: DB writes fanned out with Promise.all (partial-failure, no
// rollback). good: the writes batched in a transaction.
type Model = { create: (a: unknown) => Promise<unknown> };
declare const prisma: { a: Model; b: Model; $transaction: (ops: unknown[]) => Promise<unknown> };

export function bad() {
  return Promise.all([prisma.a.create({ data: {} }), prisma.b.create({ data: {} })]);
}

export function good() {
  return prisma.$transaction([prisma.a.create({ data: {} }), prisma.b.create({ data: {} })]);
}
