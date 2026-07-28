// DECOY for security/weak-password-hash. In scope: `.ts`, no require_file. The bcrypt call uses a cost of
// 12, and the rule's low-rounds arm matches only a SINGLE digit. The md5 call carries no credential word
// on its line — it is a file checksum, not a password hash, which is what the other two arms require.
export declare const bcrypt: { hash(v: string, rounds: number): Promise<string> };
export declare function createHash(algo: string): { update(b: Uint8Array): { digest(): string } };

export function hashPassword(plain: string): Promise<string> {
  return bcrypt.hash(plain, 12);
}

export function checksum(fileBuffer: Uint8Array): string {
  return createHash('md5').update(fileBuffer).digest();
}
