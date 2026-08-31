// be-security/jwt-sign-literal-secret — bad: the signing key is a string literal committed to source, so
// anyone who can read the repository can mint tokens. good: the key is passed in from configuration.
//
// GATE: no `require_file` on this rule — a `.ts` path is the whole gate, so this module is in scope by
// existing. What separates the two arms is the rule's own `line_pattern`: a quoted 8+ character key
// argument. `good` passes an identifier, so the pattern has nothing to match.
//
// `expiresIn` is present in BOTH arms deliberately. Without it every `jwt.sign(` here would also be a
// `security/jwt-no-expiry` co-fire, and this module is the single-rule fixture for the literal-key shape.
import * as jwt from 'jsonwebtoken';

export function bad(claims: object): string {
  return jwt.sign(claims, 'hunter2hunter2', { expiresIn: '1h' });
}

export function good(claims: object, signingKey: string): string {
  return jwt.sign(claims, signingKey, { expiresIn: '1h' });
}
