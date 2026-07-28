// be-security/cors-credentials-wildcard — bad: credentials:true alongside a wildcard origin (also trips
// be-security/cors-wildcard). This rule is FILE-level, so a `good` can't share this file (any credentials
// here would co-occur with bad's wildcard); the correct form is an explicit origin — see cors-wildcard.ts.
export const bad = { origin: '*', credentials: true };
