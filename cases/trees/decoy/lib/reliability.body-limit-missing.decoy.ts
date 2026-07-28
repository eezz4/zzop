// DECOY for reliability/body-limit-missing. In scope, provably: the rule's line_pattern
// (`(express|bodyParser)\.(json|urlencoded)\s*\(`) matches both use lines below; only its
// `exclude_pattern: limit\s*:` keeps them silent. A rule whose veto arm is never exercised is a rule
// whose precision is unmeasured.
export declare const app: { use(mw: unknown): void };
export declare const express: { json(o?: unknown): unknown; urlencoded(o?: unknown): unknown };

export function mountBodyParsers(): void {
  app.use(express.json({ limit: '1mb' }));
  app.use(express.urlencoded({ extended: true, limit: '1mb' }));
}
