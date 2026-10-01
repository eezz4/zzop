// browser/markdown-and-html-sink-unsanitized — bad: rendered markdown reaching an HTML sink with no
// sanitizer in the same function, so any raw HTML the markdown carries is executed. good: the same two
// steps with the sanitizer between them.
//
// The rule is a method-scan whose `absent` list is the sanitizer vocabulary (`DOMPurify`, `sanitize`,
// `sanitizeHtml`, `xss`), so the two halves MUST be separate functions: a sanitizer anywhere in `bad`'s
// body would silence it.
declare const marked: (md: string) => string;
declare const DOMPurify: { sanitize(html: string): string };

export function bad(md: string, el: HTMLElement): void {
  const html = marked(md);
  el.innerHTML = html;
}

export function good(md: string, el: HTMLElement): void {
  const html = marked(md);
  el.innerHTML = DOMPurify.sanitize(html);
}
