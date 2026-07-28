// DECOY for security/raw-query-unsafe-api. In scope: `.ts`, no require_file. The rule matches the
// `$queryRawUnsafe` / `$executeRawUnsafe` escape hatches only; the SAFE tagged-template forms below carry
// interpolation but parameterize it, which is exactly the distinction the rule is claiming to draw.
export declare const prisma: {
  $queryRaw(s: TemplateStringsArray, ...v: unknown[]): Promise<unknown>;
  $executeRaw(s: TemplateStringsArray, ...v: unknown[]): Promise<unknown>;
};

export function rowById(id: string): Promise<unknown> {
  return prisma.$queryRaw`SELECT id, amount_cents FROM ledger_entries WHERE id = ${id}`;
}

export function markSeen(id: string): Promise<unknown> {
  return prisma.$executeRaw`UPDATE ledger_entries SET seen = true WHERE id = ${id}`;
}
