// A source whose http consumes are majority UNRESOLVED — the URL is assembled from variables/templates,
// so key extraction fails and cross-layer rules are blind to these calls. The tree needs >= 5 total http
// consumes for the rule to report the blind spot. (2026-07-17: base-relative extraction keys 2 lines now.)
declare function fetch(u: string): Promise<unknown>;
declare const base: string;

export function calls(path: string, extra: string) {
  fetch(path); // unresolved: bare variable
  fetch(base + '/orders'); // KEYED (GET /orders): base-relative concat — joins as method-mismatch vs xbe's POST /orders
  fetch(`${base}/zz-telemetry`); // KEYED (GET /zz-telemetry): base-relative template — unique path, no provide/near-miss anywhere; unprovided-consume is off
  fetch(path + '/detail'); // unresolved: leading variable is not a recognized base carrier
  fetch(base); // unresolved
  fetch(path + extra); // unresolved: two variables, no literal to key on
  fetch(extra); // unresolved: bare variable
  fetch('/health'); // resolved literal — keyed minority; keeps the ratio "majority" not "all"
}
