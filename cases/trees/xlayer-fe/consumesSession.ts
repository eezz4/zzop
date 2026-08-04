// Consumer half of the sensitive-response escalation pair — joins GET /session/report provided by
// xlayer-be's session.controller.ts, which is what escalates that route's
// cross-layer/sensitive-response-field finding to critical (anchored at the PROVIDE side).
declare function fetch(u: string): Promise<unknown>;

export function loadSessionReport() {
  return fetch('/session/report');
}
