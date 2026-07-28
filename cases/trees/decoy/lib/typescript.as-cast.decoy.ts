// DECOY for typescript/as-cast + typescript/no-explicit-any. In scope: `.ts` matches both file_patterns,
// neither rule has a require_file, and this path is not a test path. Every line below is a NEAR MISS of
// `\bas\s+any\b` / `\bas\s+unknown\s+as\b` / `:\s*any\b` / `<any\b` / `\bany\[\]`, so silence here is a
// real precision measurement rather than an unevaluated file.
export interface LedgerRow { id: string; amountCents: number }
export type AnyRecord = Record<string, unknown>;
export type anyOfThese = 'a' | 'b';

// `as const` — explicitly vetoed by as-cast's exclude_pattern.
export const CURRENCIES = ['usd', 'eur'] as const;

// a normal narrowing cast to a named type: not `as any`, not `as unknown as`.
export function toRow(input: unknown): LedgerRow {
  return input as LedgerRow;
}

// `as unknown` NOT followed by a second `as` — the two-step laundering form is what the rule targets.
export function erase(input: LedgerRow): unknown {
  return input as unknown;
}

// `<any` is case-sensitive and needs a word boundary after `any`: `<AnyRecord` and `<anyOfThese` both
// fail it. Deliberate probes of that boundary.
export const store = new Map<AnyRecord, string>();
export const flags = new Set<anyOfThese>();

// identifiers that merely start with `any` or contain `as`.
export function hasAnyRole(roles: readonly string[]): boolean {
  return roles.length > 0;
}
export const parsedAsAnyOf: readonly string[] = [];
export const names: string[] = [];
