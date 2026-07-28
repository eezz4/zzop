// sql/race-condition-toctou (api/ path) — bad: read (findUnique) feeds a create branch. good: upsert,
// which is atomic (no check-then-act window). Also exercises be-db/find-then-create-no-unique on `bad`.
type User = {
  findUnique: (a: unknown) => Promise<unknown>;
  create: (a: unknown) => Promise<unknown>;
  upsert: (a: unknown) => Promise<unknown>;
};
declare const prisma: { user: User };

export async function bad(email: string) {
  const found = await prisma.user.findUnique({ where: { email } });
  if (!found) {
    return prisma.user.create({ data: { email } });
  }
  return found;
}

export async function good(email: string) {
  return prisma.user.upsert({ where: { email }, create: { email }, update: {} });
}
