// be-db/client-per-request — bad: new PrismaClient() inside a request handler (pool exhaustion). good: a
// module-level singleton reused across requests.
declare class PrismaClient {}
interface Req { path: string }
interface Res { json(x: unknown): void }

const shared = new PrismaClient();

export function bad(req: Req, res: Res) {
  const db = new PrismaClient(); // handler evidence = the `req.` member access below (bare res.json alone is deliberately not evidence)
  res.json({ p: req.path });
  return db;
}

export function good(_req: Req, res: Res) {
  res.json({});
  return shared;
}
