// DECOY for db/update-delete-no-where. In scope: `.ts`, no require_file — a method-scan over ORM bulk
// writes. Both calls carry an explicit `where`, which is the whole condition the rule reports the absence
// of.
export declare const prisma: {
  ledger: { updateMany(a: unknown): Promise<unknown>; deleteMany(a: unknown): Promise<unknown> };
};

export async function closeStale(cutoff: Date): Promise<void> {
  await prisma.ledger.updateMany({ where: { openedAt: { lt: cutoff } }, data: { open: false } });
}

export async function purgeClosed(): Promise<void> {
  await prisma.ledger.deleteMany({ where: { open: false } });
}
