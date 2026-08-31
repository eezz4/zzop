// be-security/jwt-none-algorithm — bad: `algorithms: ['none']` in a verify options object, which turns
// signature checking off entirely. good: the accepted algorithm pinned to a real one.
//
// GATE: the rule carries `require_file: (?i)jwt|jose|jsonwebtoken`, satisfied by the import below.
//
// Nothing here calls `jwt.sign(` — a sign call with no `expiresIn` would also be a
// `security/jwt-no-expiry`, and the literal-key arm lives in its own module next door.
import * as jwt from 'jsonwebtoken';

export function bad(raw: string, key: string): unknown {
  return jwt.verify(raw, key, { algorithms: ['none'] });
}

export function good(raw: string, key: string): unknown {
  return jwt.verify(raw, key, { algorithms: ['RS256'] });
}
