// be-db/unbounded-user-limit — bad: a page size read straight from user input. good: clamped to a max.
interface Req { query: Record<string, string> }

export function bad(req: Req) {
  return { take: Number(req.query.limit) };
}

export function good(req: Req) {
  return { take: Math.min(Number(req.query.limit), 100) };
}
