// DECOY for security/timing-unsafe-compare. In scope: `.ts`, no require_file. Each guard line matches the
// rule's `(secret|token|signature|hmac|api_key)\w*\s*[!=]==` line_pattern and is then vetoed by a
// different arm of its exclude_pattern: nullish comparison, typeof guard, string-literal comparison.
export declare function timingSafeEqual(a: Uint8Array, b: Uint8Array): boolean;

export function guard(token?: string, secret?: unknown, apiKey?: string): boolean {
  if (token === undefined) return false;
  if (typeof secret !== 'string') return false;
  if (apiKey === '') return false;
  return true;
}

// the correct form: a constant-time comparison, which carries no `===` at all.
export function compareSignature(a: Uint8Array, b: Uint8Array): boolean {
  return timingSafeEqual(a, b);
}
