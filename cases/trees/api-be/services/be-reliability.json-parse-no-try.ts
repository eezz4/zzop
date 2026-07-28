// be-reliability/json-parse-no-try — bad: JSON.parse on request input with no try. good: guarded parse
// that returns a 400 on malformed input.
interface Req { body: string }
interface Res { json(x: unknown): void; status(n: number): Res; send(x: unknown): void }

export function bad(req: Req, res: Res) {
  const body = JSON.parse(req.body);
  res.json(body);
}

export function good(req: Req, res: Res) {
  try {
    const body = JSON.parse(req.body);
    res.json(body);
  } catch {
    res.status(400).send('invalid json');
  }
}
