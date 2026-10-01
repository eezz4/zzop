// be-security/jwt-verify-bypass — bad: verification asked to ignore the expiry, which turns a token with
// a lifetime into a token without one — a leaked token stays valid forever. good: the expiry honoured,
// with an explicit maximum age.
//
// GATE: the rule carries `require_file: (?i)jsonwebtoken|jose|jwt`, satisfied by the import below.
//
// The algorithm is pinned in BOTH halves on purpose: leaving it out would make each line a
// `security/jwt-none-algorithm` question as well, and that shape has its own fixture next door.
import * as jwt from 'jsonwebtoken';

export function bad(raw: string, key: string): unknown {
  return jwt.verify(raw, key, { algorithms: ['RS256'], ignoreExpiration: true });
}

// `ignoreExpiration: false`, not some other option (review ledger V335). The matcher is
// `ignoreExpiration\s*:\s*true`, so the nearest NON-defect is the same key with the other value —
// a widening to the key name alone reports this line, which is the regression worth catching. A
// `maxAge` control, which is what stood here, shares no token with the pattern and proved nothing.
export function good(raw: string, key: string): unknown {
  return jwt.verify(raw, key, { algorithms: ['RS256'], ignoreExpiration: false, maxAge: '15m' });
}
