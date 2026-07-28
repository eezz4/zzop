// be-reliability/sync-fs-in-handler — bad: a synchronous fs call in a request handler. good: the async
// equivalent (does not block the event loop).
import * as fs from 'fs';
interface Req { path: string }
interface Res { send(x: unknown): void }

export function bad(req: Req, res: Res) {
  const t = fs.readFileSync('/tmp/x.html', 'utf8');
  res.send(t);
}

export async function good(req: Req, res: Res) {
  const t = await fs.promises.readFile('/tmp/x.html', 'utf8');
  res.send(t);
}
