// sql/race-condition-toctou (TOP-LEVEL routes/ path) — the 2026-08-03 anchor alignment. The routes/
// and controllers/ arms used to spell `.*/routes/` — at least one directory ABOVE them — while the
// api/ arm was `(?:^|/)`, so this tree-root routes/ directory (the layout express-generator
// scaffolds) was silently out of scope — `.*/routes/` structurally cannot match a path with no `/`
// before `routes/`. All three directory arms now share `(?:^|/)`; reverting the
// alignment turns this file's labels into FN 2. bad: read (findUnique) feeds a create branch.
// good: upsert, which is atomic. Also exercises be-db/find-then-create-no-unique on `bad`, same as
// the api/ sibling fixture api/sql.race-condition-toctou.ts.
type Member = {
  findUnique: (a: unknown) => Promise<unknown>;
  create: (a: unknown) => Promise<unknown>;
  upsert: (a: unknown) => Promise<unknown>;
};
declare const prisma: { member: Member };

export async function bad(orgId: string) {
  const found = await prisma.member.findUnique({ where: { orgId } });
  if (!found) {
    return prisma.member.create({ data: { orgId } });
  }
  return found;
}

export async function good(orgId: string) {
  return prisma.member.upsert({ where: { orgId }, create: { orgId }, update: {} });
}
