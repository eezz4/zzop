// DECOY for sql/race-condition-toctou + db/find-then-create-no-unique. In scope, provably: the rule's
// file_pattern needs an `api/`, `routes/`, `controllers/`, `handler` or `controller` path — this file is
// under `api/` — and its require_file (`\.(findOne|findById|findUnique)\s*\(`) is satisfied. What is
// missing is the second half of the pattern: no write follows the read, so there is nothing to race.
export declare const prisma: {
  customer: { findUnique(a: unknown): Promise<CustomerRow | null> };
};
export interface CustomerRow { id: string; email: string }

export async function readCustomer(id: string): Promise<CustomerRow | null> {
  return prisma.customer.findUnique({ where: { id } });
}
