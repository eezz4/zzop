// be-reliability/env-outside-config — bad: process.env read outside a config module. good: read from an
// injected config object (env parsing centralized elsewhere).
export function bad() {
  return process.env.DATABASE_URL;
}

export function good(config: { databaseUrl: string }) {
  return config.databaseUrl;
}

// Keeps the declared env-config module reachable — a config module nothing imports is dead by
// construction, and the dead-code rules are right to say so.
export { env } from "../config/env";
