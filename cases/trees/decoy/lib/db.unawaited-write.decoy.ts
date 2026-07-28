// DECOY for db/unawaited-write. In scope, provably: every write line below matches the rule's
// line_pattern (`(prisma|db|tx|client|repo…).<model>.(create|update|delete|upsert)(`), and each is then
// vetoed by a different arm of its exclude_pattern — awaited, returned, assigned, or `.then`-chained.
export declare const prisma: {
  ledger: { create(a: unknown): Promise<unknown>; upsert(a: unknown): Promise<unknown> };
  order: { update(a: unknown): Promise<unknown> };
  audit: { create(a: unknown): Promise<unknown> };
};

export async function writeAwaited(data: unknown): Promise<void> {
  await prisma.ledger.create({ data });
}

export function writeReturned(id: string, data: unknown): Promise<unknown> {
  return prisma.order.update({ where: { id }, data });
}

export function writeAssigned(data: unknown): Promise<unknown> {
  const pending = prisma.ledger.upsert({ create: data, update: data });
  return pending;
}

export function writeChained(data: unknown): void {
  prisma.audit.create({ data }).then(() => undefined);
}
