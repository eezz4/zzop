// be-db/unawaited-write — bad: a fire-and-forget DB write. good: the write awaited.
type Event = { create: (a: unknown) => Promise<unknown> };
declare const prisma: { event: Event };

export function bad() {
  prisma.event.create({ data: { kind: 'x' } });
}

export async function good() {
  await prisma.event.create({ data: { kind: 'x' } });
}

// good (regression tripwire, unlabeled): a formatter-wrapped concise arrow body RETURNS the
// promise — callers await it. The `=>` ends the previous line, where a same-line exclusion
// cannot see it; if the rule ever regresses, this surfaces as an unexpected FP in the gate.
export const persistEvent = (data: { kind: string }) =>
  prisma.event.create({ data });
