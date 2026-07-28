// DECOY for reliability/json-parse-no-try. In scope, provably: `.ts` matches, and the rule's trigger
// pattern is narrow — `JSON.parse(` applied to a REQUEST-shaped identifier (`req|request|body|params|
// query|payload|message|event`), not to any old string. Both parses below match that trigger, so the rule
// really engaged; what silences them is its `absent: \btry\s*\{` guard.
//
// The first draft of this decoy parsed a plain `raw` string, which the trigger never matches at all — the
// file was technically in scope but the matcher never engaged, so its silence measured nothing. Caught by
// the scope-verification pass, not by reading.
export function parseBody(body: string): unknown {
  try {
    return JSON.parse(body);
  } catch {
    return null;
  }
}

export function parseRequest(req: { raw: string }, contentType: string): unknown {
  if (contentType !== 'application/json') return undefined;
  try {
    return JSON.parse(req.raw);
  } catch {
    return undefined;
  }
}
