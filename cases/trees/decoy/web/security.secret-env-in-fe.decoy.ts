// DECOY for security/secret-env-in-fe. In scope, provably: the rule's file_pattern admits a `.ts` file
// only under an `fe|frontend|client|web` path segment, and this file sits under `web/` — the same file in
// `lib/` would never be evaluated. Its line_pattern needs a SECRET/PRIVATE/SERVICE_ROLE/SERVICE_KEY
// fragment in the variable name; both reads below are explicitly public-prefixed values.
export const apiBase = process.env.NEXT_PUBLIC_API_BASE ?? '';
export const title = import.meta.env.VITE_PUBLIC_TITLE ?? 'Ledger';
