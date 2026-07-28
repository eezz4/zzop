// be-security/raw-query-interpolation — bad: $queryRawUnsafe with a concatenated string. good: a
// parameterized tagged-template $queryRaw.
type Db = {
  $queryRawUnsafe: (s: string) => Promise<unknown>;
  $queryRaw: (s: TemplateStringsArray, ...v: unknown[]) => Promise<unknown>;
};
declare const prisma: Db;

export function bad(name: string) {
  return prisma.$queryRawUnsafe('SELECT * FROM users WHERE name = ' + name);
}

export function good(name: string) {
  return prisma.$queryRaw`SELECT * FROM users WHERE name = ${name}`;
}
