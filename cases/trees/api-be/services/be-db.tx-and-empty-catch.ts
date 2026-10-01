// be-db/tx-and-empty-catch — bad: a transaction whose failure is swallowed whole. The caller is told the
// work succeeded and the rows were never written. good: the same transaction with the failure re-raised
// after it is recorded.
type Client = { $transaction: (ops: readonly unknown[]) => Promise<unknown> };
declare const prisma: Client;
declare const logger: { error(e: unknown): void };
declare const opA: unknown;

export async function bad(): Promise<void> {
  try {
    await prisma.$transaction([opA]);
  } catch (e) {}
}

export async function good(): Promise<void> {
  try {
    await prisma.$transaction([opA]);
  } catch (e) {
    logger.error(e);
    throw e;
  }
}
