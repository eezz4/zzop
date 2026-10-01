// browser/javascript-url — bad: a `javascript:` URL assigned to `.href`, which executes whatever follows
// the scheme in the page's origin. good: a real path.
//
// GATE: the rule carries `require_file: (?i)javascript:`, satisfied by the bad line. The rule has three
// arms (`jsx-href-literal`, `href-assign`, `setattr-js`); this file exercises `href-assign`, the one
// whose sink is an assignment rather than markup, because the other two need a JSX or DOM-API context
// that would pull unrelated rules into the same fixture.
declare const anchor: { href: string };

export function bad(): void {
  anchor.href = 'javascript:void(0)';
}

// The good half carries the WORD `javascript` in a NON-SCHEME position (review ledger V335). A plain
// `/dashboard` avoided the scheme and also avoided the vocabulary, so it was a control for nothing: a
// matcher widened from `javascript:` to `javascript` — the obvious loosening — would not have touched
// it. This line is what such a widening reports, and the rule must stay silent on it.
export function good(): void {
  anchor.href = '/docs/javascript-guide';
}
