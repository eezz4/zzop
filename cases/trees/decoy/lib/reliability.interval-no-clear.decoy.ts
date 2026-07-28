// DECOY for reliability/interval-no-clear. In scope, provably: `.ts` matches, and the rule's line_pattern
// (`\bsetInterval\s*\(`) DOES match line 8 — the finding is suppressed only by the rule's
// `require_file_absent: clearInterval`, which this file satisfies. That makes it a test of the rule's
// veto arm rather than of its regex.
export declare function setInterval(fn: () => void, ms: number): number;
export declare function clearInterval(h: number): void;

export function startHeartbeat(tick: () => void): () => void {
  const handle = setInterval(tick, 1000);
  return () => clearInterval(handle);
}
