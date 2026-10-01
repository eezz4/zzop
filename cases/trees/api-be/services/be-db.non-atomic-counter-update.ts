// be-db/non-atomic-counter-update — bad: read the row, add one in application memory, write it back.
// Two concurrent requests read the same value and one increment is lost. good: let the database do the
// arithmetic, which is a single atomic statement.
//
// The rule is a method-scan whose `absent` list is the atomic vocabulary (`increment:`, `decrement:`,
// `$inc`, `FOR UPDATE`), so `good` has to be its own function — an `increment:` anywhere in `bad`'s body
// would silence it.
type Counter = {
  findUnique: (a: unknown) => Promise<{ hits: number }>;
  update: (a: unknown) => Promise<unknown>;
};
declare const prisma: { counter: Counter };

export async function bad(id: string): Promise<unknown> {
  const row = await prisma.counter.findUnique({ where: { id } });
  return prisma.counter.update({ where: { id }, data: { hits: row.hits + 1 } });
}

export async function good(id: string): Promise<unknown> {
  return prisma.counter.update({ where: { id }, data: { hits: { increment: 1 } } });
}
