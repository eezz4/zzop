// be-security/jwt-no-expiry — bad: jwt.sign with no expiresIn. good: an explicit expiry.
import jwt from 'jsonwebtoken';

export function bad(userId: string) {
  return jwt.sign({ sub: userId }, 'signing-key');
}

export function good(userId: string) {
  return jwt.sign({ sub: userId }, 'signing-key', { expiresIn: '1h' });
}
