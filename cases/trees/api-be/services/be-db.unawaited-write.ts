// be-db/unawaited-write — bad: a fire-and-forget DB write. good: the write awaited.
type Event = { create: (a: unknown) => Promise<unknown> };
declare const prisma: { event: Event };

export function bad() {
  prisma.event.create({ data: { kind: 'x' } });
}

export async function good() {
  await prisma.event.create({ data: { kind: 'x' } });
}
