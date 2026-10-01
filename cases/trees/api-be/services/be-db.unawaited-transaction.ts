// be-db/unawaited-transaction — bad: the transaction promise is neither awaited, returned, assigned nor
// given a `.then`/`.catch`, so the handler can answer before the transaction commits and a rejection
// surfaces as an unhandled promise. good: the same call awaited.
type Client = { $transaction: (ops: readonly unknown[]) => Promise<unknown> };
declare const prisma: Client;
declare const opA: unknown;
declare const opB: unknown;

export async function bad(): Promise<void> {
  prisma.$transaction([opA, opB]);
}

export async function good(): Promise<void> {
  await prisma.$transaction([opA, opB]);
}
