// DECOY for security/weak-password-hash. In scope: `.ts`, no require_file. The bcrypt call uses a cost of
// 12, and the rule's low-rounds arm matches only a SINGLE digit. The md5 call carries no credential word
// on its line — it is a file checksum, not a password hash, which is what the other two arms require.
// The third control is the NEGATIVE-GUARD shape (added 2026-08-02 with the rule's critical->warning
// demotion): a guard that REJECTS md5/sha1 names the algorithm and `password` on one line, and stays
// silent only because the statement's `;` (inside the error string, before `password`) trips the arms'
// `[^;]*` statement-boundary heuristic. Unit twin: rules/dsl/security/crypto.rs
// `negative_guard_line_rejecting_md5_is_not_flagged`.
export declare const bcrypt: { hash(v: string, rounds: number): Promise<string> };
export declare function createHash(algo: string): { update(b: Uint8Array): { digest(): string } };

export function hashPassword(plain: string): Promise<string> {
  return bcrypt.hash(plain, 12);
}

export function checksum(fileBuffer: Uint8Array): string {
  return createHash('md5').update(fileBuffer).digest();
}

export function assertStrongHashAlgo(algo: string): void {
  if (algo === 'md5' || algo === 'sha1') throw new Error('weak digest rejected; use bcrypt for password hashing');
}
