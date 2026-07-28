// be-security/secret-env-in-fe — bad: a server-only-shaped env var referenced from frontend code (gets
// inlined into the shipped bundle). good: config injected at build/runtime via a plain object.
export const bad = process.env.SERVICE_ROLE_KEY;

export function good(config: { apiUrl: string }) {
  return config.apiUrl;
}

// Keeps the declared env-config module reachable (see api-be's twin).
export { env } from "../config/env";
