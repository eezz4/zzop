// be-security/timing-unsafe-compare — bad: a secret compared with ===. good: constant-time comparison.
import { timingSafeEqual } from 'crypto';

export function bad(apiKey: string, provided: string) {
  return apiKey === provided;
}

export function good(apiKey: string, provided: string) {
  return timingSafeEqual(Buffer.from(apiKey), Buffer.from(provided));
}
