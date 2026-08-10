// security/insecure-cookie on the span-boundary axis, FN DIRECTION (see ./README.md). Another
// C-veto-only rule, and its veto is a single token — `httpOnly: true` — which makes the failure mode
// concrete: one correctly-configured cookie anywhere in the span clears every other cookie in it.
//
// FN PROBE: this is the ordinary shape of a real cookie writer, several cookies set from one class.
// `writeRefreshCookie` is correct. `writeSessionCookie` forgot `httpOnly`, so the session id is
// readable by any script on the page — the exact defect the rule names. If it goes silent because its
// sibling is correct, the rule cannot see the common case it was written for.

interface CookieResponse {
  cookie(name: string, value: string, options: Record<string, unknown>): void;
}

export class AuthCookieWriter {
  private readonly base = { sameSite: 'lax' as const, secure: true, path: '/' };

  writeRefreshCookie = (res: CookieResponse, token: string) => {
    res.cookie('rt', token, { ...this.base, httpOnly: true, maxAge: 1209600 });
  };

  writeSessionCookie = (res: CookieResponse, token: string) => {
    res.cookie('sid', token, { ...this.base, maxAge: 3600 });
  };
}

// TP CONTROL — the same missing-`httpOnly` cookie in its own function span. This must fire.
export function writeLegacySessionCookie(res: CookieResponse, token: string) {
  res.cookie('sid', token, { sameSite: 'lax', secure: true, maxAge: 3600 });
}
