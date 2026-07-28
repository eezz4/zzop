// be-security/mass-assignment — bad: req.body written straight into an update. good: an explicit,
// allow-listed field.
type User = { update: (a: unknown) => Promise<unknown> };
declare const prisma: { user: User };
interface Req { body: { name: string }; params: Record<string, string> }

export function bad(req: Req) {
  return prisma.user.update({ where: { id: req.params.id }, data: req.body });
}

export function good(req: Req) {
  return prisma.user.update({ where: { id: req.params.id }, data: { name: req.body.name } });
}
