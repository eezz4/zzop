// http/protected-path-no-auth-evidence — bad: an /admin/ route with no witnessed auth evidence. The
// matcher is an io-scan over `provides` keyed on /(admin|internal)/ with attr_absent:"auth-guarded"
// (rules/dsl/http/http.json). It NEVER reads the handler identifier, so a `requireAdmin…`-shaped name
// clears nothing; the ONLY clearing mechanism is an injected `auth-guarded` attribute. Both good routes
// below get one from the Mode B adapter overlay ../../zzop-attributes.json — one via an exact IoKey
// target, one via a PathScope prefix. (Sibling rule dev-path-no-guard-hint clears its own good example by
// keyword-matching the registration LINE instead. That asymmetry is a rules-owner question, not a defect.)
declare const apiRoutes: { get(path: string, handler: unknown): void };
declare const listUsers: unknown;
declare const adminReports: unknown;
declare const auditEvents: unknown;

export function bad() {
  apiRoutes.get('/admin/users', listUsers); // cache: server-render (isolates this from get-route-no-cache-marker)
}

// NEGATIVE CONTROL 1 — cleared by an EXACT IoKey attribute on `GET /admin/reports`.
export function good() {
  apiRoutes.get('/admin/reports', adminReports); // cache: server-render
}

// NEGATIVE CONTROL 2 — cleared by a PathScope attribute on prefix `/admin/audit`. Segment-boundary match,
// so it covers this route and NOT the `/admin/users` bad route above.
export function goodPathScope() {
  apiRoutes.get('/admin/audit/events', auditEvents); // cache: server-render
}
