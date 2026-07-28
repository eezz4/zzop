// DECOY for security/jwt-none-algorithm. In scope, provably: the rule's require_file (`jwt|jose|
// jsonwebtoken`, case-insensitive) is satisfied by the `jwt` identifier below, so the file was scanned.
// Its line_pattern needs the literal algorithm `none`, and both call sites pin a real algorithm instead.
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
