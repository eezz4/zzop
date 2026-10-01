// browser/postmessage-wildcard — bad: a cross-window message sent with `'*'` as the target origin, so
// any document that can obtain a handle to the window receives the payload. good: the exact origin the
// receiver is expected to be served from.
//
// GATE: the rule carries `require_file: postMessage\s*\(`, satisfied by both calls below.
declare const frame: { contentWindow: { postMessage(m: unknown, origin: string): void } };

export function bad(payload: unknown): void {
  frame.contentWindow.postMessage(payload, '*');
}

export function good(payload: unknown): void {
  frame.contentWindow.postMessage(payload, 'https://app.example.com');
}
