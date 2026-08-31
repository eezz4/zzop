// DECOY for security/dangerous-html-concat. In scope, provably: the rule's require_file
// (`(?i)(res\.|response\.|content-type|text/html)`) is satisfied by the `res.` calls below, so the file
// was scanned rather than skipped by a gate.
//
// This decoy measures the LINE_PATTERN arm, both halves of it. The rule's two patterns are
// `open-tag-concat` (a string literal that STARTS with `<tag`, concatenated with an identifier) and
// `close-tag-concat` (an identifier concatenated with a string literal CONTAINING a `<...>` tag). Every
// concatenation here is ordinary text assembly: no string in this file opens with `<`, and none contains
// a tag. Widen either pattern to "any concatenation reaching a response" and this file starts firing.
declare const res: { send(body: unknown): void };

export function summary(total: number, currency: string) {
  res.send('Total: ' + total + ' ' + currency);
}

export function greeting(name: string) {
  res.send(name + ' is signed in');
}
