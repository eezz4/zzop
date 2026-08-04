// be-reliability/env-outside-config — bad: process.env read outside a config module. good: read from an
// injected config object (env parsing centralized elsewhere).
export function bad() {
  return process.env.DATABASE_URL;
}

export function good(config: { databaseUrl: string }) {
  return config.databaseUrl;
}

// A second positive in the same module, kept here rather than in a file of its own so the rule keeps its
// one-fixture-per-rule naming. This is the READ SHAPE the rule could not see before it moved onto the
// projected call-site channel: the regex it used to run (`\bprocess\.env\.[A-Za-z0-9_]+`) required a dot
// followed by a bare identifier, so a quoted key and a computed key were both invisible to it. The
// producer emits for both, because the callee is fully resolved and only the KEY is dynamic — and the key
// is not a field of this channel, so emitting guesses nothing. Two reads, two findings.
export function badBracketKeys(name: string) {
  const explicit = process.env["API_KEY"];
  const computed = process.env[name];
  return explicit ?? computed;
}

// Keeps the declared env-config module reachable — a config module nothing imports is dead by
// construction, and the dead-code rules are right to say so.
export { env } from "../config/env";
