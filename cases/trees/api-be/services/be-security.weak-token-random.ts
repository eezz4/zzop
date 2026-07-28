// be-security/weak-token-random — bad: Math.random() for a token. good: a CSPRNG.
import { randomBytes } from 'crypto';

export function bad() {
  const token = Math.random().toString(36).slice(2);
  return token;
}

export function good() {
  const token = randomBytes(32).toString('hex');
  return token;
}
