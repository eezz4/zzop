// DECOY for sql/nplus1. In scope, provably: the rule's file_pattern is `^(?:domains/[^/]+/routes/.+|api/.+)\.ts$`
// against the TREE-RELATIVE path, which `api/sql.nplus1.decoy.ts` satisfies, and its require_file (`\bawait\b`)
// is satisfied too. The fix-shape is what is planted here: one batched round trip, then a pure in-memory
// loop over the result. A decoy placed outside `api/` would prove nothing, because the rule never looks.
export declare const prisma: {
  order: { findMany(a?: unknown): Promise<OrderRow[]> };
};
export interface OrderRow { id: string; customerId: string; amountCents: number }

export async function totalsForCustomers(customerIds: readonly string[]): Promise<number> {
  const rows = await prisma.order.findMany({ where: { customerId: { in: customerIds } } });
  let total = 0;
  for (const row of rows) {
    total += row.amountCents;
  }
  return total;
}
