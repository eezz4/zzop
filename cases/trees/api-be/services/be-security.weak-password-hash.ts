// be-security/weak-password-hash — bad: MD5 for a password. good: bcrypt at a sane cost factor.
import { createHash } from 'crypto';
import bcrypt from 'bcrypt';

export function bad(password: string) {
  return createHash('md5').update(password).digest('hex');
}

export function good(password: string) {
  return bcrypt.hash(password, 12);
}
