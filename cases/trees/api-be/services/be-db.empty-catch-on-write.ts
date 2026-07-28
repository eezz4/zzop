// be-db/empty-catch-on-write — bad: a DB write whose failure is swallowed by an empty catch. good: the
// catch handles/logs the failure.
type Model = { create: (a: unknown) => Promise<unknown> };
declare const prisma: { event: Model };
declare const logger: { error(e: unknown): void };

export async function bad() {
  try {
    await prisma.event.create({ data: {} });
  } catch {
    // swallowed
  }
}

export async function good() {
  try {
    await prisma.event.create({ data: {} });
  } catch (e) {
    logger.error(e);
  }
}
