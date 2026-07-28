// http/dev-path-no-guard-hint — bad: a /debug/ route whose handler name carries no guard-hint keyword.
// good: the handler identifier signals a guard.
declare const apiRoutes: { get(path: string, handler: unknown): void };
declare const dumpState: unknown;
declare const guardedDumpState: unknown;

export function bad() {
  apiRoutes.get('/debug/state', dumpState); // cache: server-render (isolates this from get-route-no-cache-marker)
}

export function good() {
  apiRoutes.get('/debug/metrics', guardedDumpState); // cache: server-render
}
