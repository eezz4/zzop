// security/weak-crypto (TypeScript lane) — MUST FIRE on the createHash('md5') line (EXPECTED ts:23).
//
// HISTORY: written BEFORE the multi-language migration as a labeled gap — the rule then read Java
// only, so this file was expected absent and its line lived in EXPECTED.jsonc's `gap` object. The
// migration landed 2026-08-09: weak-crypto now rides the parser's `hash-call` call-site channel
// across six languages, all three sibling lanes (.ts/.py/.go) fired on the first post-migration
// run, and the entries were promoted to ordinary scored must-fire expectations (`gap` is {} again).
//
// bad: MD5 over a cache key. Not a password hash — `security/weak-password-hash` requires a
// credential word on the call line and correctly stays silent here; judging this non-credential
// class of use is exactly what the migration bought. MD5's collision weakness matters for a cache
// key too: two different requests can be made to collide onto one entry, which is a cache-poisoning
// primitive.
// good: SHA-256, the same call shape with a strong algorithm — must stay silent.

import { createHash } from 'node:crypto';

export function cacheKeyForRoute(route: string, params: Record<string, string>): string {
  const canonical = `${route}?${Object.keys(params)
    .sort()
    .map((k) => `${k}=${params[k]}`)
    .join('&')}`;
  return createHash('md5').update(canonical).digest('hex');
}

export function goodCacheKeyForRoute(route: string, params: Record<string, string>): string {
  const canonical = `${route}?${Object.keys(params)
    .sort()
    .map((k) => `${k}=${params[k]}`)
    .join('&')}`;
  return createHash('sha256').update(canonical).digest('hex');
}
