// security/taint-flow — bad: a tainted source (request input) reaches a dangerous sink (exec) in one
// function. good: a fixed, non-tainted command.
declare function exec(cmd: string): void;
interface Req { query: Record<string, string> }

export function bad(req: Req) {
  const cmd = req.query.cmd;
  exec(cmd);
}

export function good() {
  exec('ls -la /srv');
}
