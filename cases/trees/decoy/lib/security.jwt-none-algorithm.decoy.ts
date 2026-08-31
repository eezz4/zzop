// DECOY for security/jwt-none-algorithm AND security/jwt-sign-literal-secret — two rules, because this
// one file is in scope for both and silent for both by two different mechanisms.
//
// security/jwt-none-algorithm — in scope, provably: the rule's require_file (`jwt|jose|jsonwebtoken`,
// case-insensitive) is satisfied by the `jwt` identifier below, so the file was scanned. Its line_pattern
// needs the literal algorithm `none`, and both call sites pin a real algorithm instead.
//
// security/jwt-sign-literal-secret — in scope by extension alone (that rule carries no require_file, so a
// `.ts` path is its whole gate). Its line_pattern needs a quoted 8-or-more-character key ARGUMENT to
// `jwt.sign(`; `issueToken` passes an identifier, and the two short literals on that line (`'HS256'`,
// `'15m'`) are below the length floor. Drop that floor and this line reports.
export declare const jwt: {
  verify(t: string, key: string, opts: unknown): unknown;
  sign(p: unknown, key: string, opts: unknown): string;
};

export function verifyToken(raw: string, key: string): unknown {
  return jwt.verify(raw, key, { algorithms: ['RS256'] });
}

export function issueToken(payload: unknown, key: string): string {
  return jwt.sign(payload, key, { algorithm: 'HS256', expiresIn: '15m' });
}
