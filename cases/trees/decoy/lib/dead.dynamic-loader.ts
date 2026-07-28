// The dynamic-import hop for `dead.dynamic-import.decoy.ts`. Deliberately `.then`-chained rather than
// awaited, so this file carries no `await` and therefore does not enter the scope of the await-gated rules
// — the decoy under test is the dep-graph edge, not error handling.
export function loadPanel(): Promise<string> {
  return import('./dead.dynamic-import.decoy').then((m) => m.lazilyLoadedPanel());
}
