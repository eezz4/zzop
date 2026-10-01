// be-redis/lock-no-ttl — bad: a lock key claimed with no expiry, so a holder that crashes before
// releasing it blocks every later holder forever. good: the same claim with an expiry, which bounds the
// outage to the TTL.
//
// GATE: the rule carries `require_file: (?i)redis|ioredis`, satisfied by the declaration below.
//
// The client is a bare `declare const redis: any` with no `new Redis(`/`createClient(` call, for the
// reason be-redis.flushall-in-code.ts states: a constructor here would also make this module a
// `redis/client-no-error-listener`.
//
// MEASURED while writing this file, and the reason the client is `any` rather than a typed shape: a
// TYPE DECLARATION spelling `setnx(key: string, value: string): Promise<number>;` fires this rule. The
// matcher is `\bsetnx\s*\(` on a line, and a method signature in an interface is neither a comment
// nor a call, so nothing separates it from the real thing. That residual is recorded rather than
// silenced here — a veto for declaration lines would be a widening, and a widening can only ever
// remove findings, so it needs evidence from real code and not from a fixture written to provoke it.
declare const redis: any;

export async function bad(token: string): Promise<number> {
  return redis.setnx('lock:invoice', token);
}

export async function good(token: string): Promise<unknown> {
  return redis.set('lock:invoice', token, 'NX', 'EX', 30);
}
