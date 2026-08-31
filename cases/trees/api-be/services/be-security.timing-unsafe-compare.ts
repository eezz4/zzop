// be-security/timing-unsafe-compare — bad: a secret compared with ===. good: fixed-width digests compared in constant time.
import { createHash, timingSafeEqual } from 'crypto';

export function bad(apiKey: string, provided: string) {
  return apiKey === provided;
}

// `timingSafeEqual` is not a drop-in for `===` on these two values. It throws TypeError on plain
// strings and RangeError when the byte lengths differ, and a stored secret versus a request-supplied
// value can differ in length — so the literal substitution turns every wrong guess into a 500.
// Hashing each side first makes both operands 32 bytes for any input, so the comparison returns false
// where the direct call would have thrown.
const digest = (s: string) => createHash('sha256').update(s).digest();

export function good(apiKey: string, provided: string) {
  return timingSafeEqual(digest(apiKey), digest(provided));
}
