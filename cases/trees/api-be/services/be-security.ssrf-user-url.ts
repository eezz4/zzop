// be-security/ssrf-user-url — bad: an outbound call whose URL comes from request input. good: a fixed,
// server-controlled URL (the user never chooses the host).
declare function fetch(u: string): Promise<unknown>;
interface Req { query: Record<string, string> }

export function bad(req: Req) {
  return fetch(req.query.target);
}

export function good() {
  return fetch('https://svc.example.com/report');
}
