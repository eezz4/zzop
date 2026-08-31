// be-security/dangerous-html-concat — bad: an HTML tag literal concatenated with a variable on its way
// into a response body. good: the same value handed to a template engine, which escapes it.
//
// GATE: the rule carries `require_file: (?i)(res\.|response\.|content-type|text/html)`, so it never runs
// on a file with no response receiver in it. The `res.` calls below are what put this module in scope.
//
// No request-derived value appears here on purpose. With one, the bad line would ALSO be a
// `security/html-response-from-request` (that rule needs the request input this one does not), and this
// module is the single-rule fixture for the concatenation shape on its own.
declare const res: {
  send(body: unknown): void;
  render(view: string, locals: unknown): void;
};

export function bad(name: string) {
  res.send('<div>' + name);
}

export function good(name: string) {
  res.render('greet', { name });
}
