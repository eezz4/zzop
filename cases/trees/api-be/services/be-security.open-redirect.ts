// be-security/open-redirect — bad: redirect target comes from request input. good: a fixed internal path.
interface Req { query: Record<string, string> }
interface Res { redirect(url: string): void }

export function bad(req: Req, res: Res) {
  res.redirect(req.query.next);
}

export function good(_req: Req, res: Res) {
  res.redirect('/dashboard');
}
