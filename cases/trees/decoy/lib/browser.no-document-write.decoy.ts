// DECOY for browser/no-document-write. In scope, provably: the rule's require_file
// (`\bdocument\.(?:write|writeln)\s*\(`) is satisfied by the call below, so the file was scanned. Its
// line_pattern additionally forbids a preceding `.`/word character, so writing into an OWNED iframe
// document — the one legitimate use — is out of the rule's stated scope.
export declare const iframeDoc: { document: { write(html: string): void } };

export function renderIntoFrame(html: string): void {
  iframeDoc.document.write(html);
}
