// DECOY for security/weak-token-random. In scope: `.ts`, no require_file. The rule needs a credential
// word and `Math.random()` on the SAME line; these keep the two apart, which is the shape correct code has.
export declare function randomBytes(n: number): { toString(enc: string): string };

export function jitterMs(): number {
  return Math.floor(Math.random() * 100);
}

export function newToken(): string {
  return randomBytes(32).toString('hex');
}
