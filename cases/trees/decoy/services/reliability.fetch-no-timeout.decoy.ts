// DECOY for reliability/fetch-no-timeout. In scope, provably: the rule's require_file is a SERVER signal
// (express import, `createServer(`, `.listen(<digit>`, `.prepare(`, …) and `app.listen(3000)` below
// supplies it — without that line the rule would never look at this file and the decoy would prove
// nothing. The fetch then carries an explicit AbortSignal deadline, which is the fix the rule asks for.
//
// This is the ONLY outbound call in the decoy tree, on a host used nowhere else in the corpus and with a
// three-segment path, so it cannot pull in cross-layer/external-host-fanout (needs several files on one
// host), external-duplicated-integration (needs two source trees on one host) or external-base-url-drift
// (needs sibling hosts sharing a two-segment path).
export declare const app: { listen(port: number): void };
export declare function fetch(url: string, init?: unknown): Promise<{ json(): Promise<unknown> }>;
export declare const AbortSignal: { timeout(ms: number): unknown };

app.listen(3000);

export async function fetchQuote(): Promise<unknown> {
  try {
    const res = await fetch('https://decoy-quotes.example.net/v1/quotes/latest', {
      signal: AbortSignal.timeout(5000),
    });
    return await res.json();
  } catch {
    return null;
  }
}
