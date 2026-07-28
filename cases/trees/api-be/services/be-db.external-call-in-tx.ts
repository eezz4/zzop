// be-db/external-call-in-tx — bad: a network call inside a $transaction (holds the lock across a round
// trip). good: the transaction with no network call in it (do the fetch before/after).
declare const prisma: { $transaction: (fn: () => Promise<unknown>) => Promise<unknown> };
declare function fetch(u: string): Promise<unknown>;

export function bad() {
  return prisma.$transaction(async () => {
    await fetch('https://svc.example.com/x');
  });
}

export function good() {
  return prisma.$transaction(async () => {
    /* only DB work here; network calls happen outside the transaction */
  });
}
