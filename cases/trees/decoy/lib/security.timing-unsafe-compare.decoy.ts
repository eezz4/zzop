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

// the correct form ONCE BOTH SIDES ARE EQUAL-LENGTH BYTES: a constant-time comparison, which carries
// no `===` at all. `timingSafeEqual` throws RangeError on a length mismatch and TypeError on strings,
// so a caller holding a stored secret and a request-supplied value digests each side to a fixed width
// first — see cases/trees/api-be/services/be-security.timing-unsafe-compare.ts for that shape.
export function compareSignature(a: Uint8Array, b: Uint8Array): boolean {
  return timingSafeEqual(a, b);
}
