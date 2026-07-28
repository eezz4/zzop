// be-security/insecure-cookie — bad: a cookie set with no httpOnly. good: httpOnly + secure.
interface Res { cookie(name: string, value: string, opts?: unknown): void }

export function bad(res: Res, token: string) {
  res.cookie('sid', token);
}

export function good(res: Res, token: string) {
  res.cookie('sid', token, { httpOnly: true, secure: true, sameSite: 'lax' });
}
