// DECOY for typescript/parseint-no-radix. In scope: `.ts`, and the rule's require_file (`parseInt`) is
// satisfied by every line below — the rule really did evaluate this file. Its line_pattern is
// `\b(?:Number\.)?parseInt\s*\(\s*[^,()]+\)`, which can cross neither a comma nor a nested paren.
export function withRadix(raw: string): number {
  return parseInt(raw, 10);
}
export function withNamespaceAndRadix(hex: string): number {
  return Number.parseInt(hex, 16);
}
// nested call in the first argument: `[^,()]+` cannot cross the inner `(`.
export function nestedArgument(n: number): number {
  return parseInt(String(n), 10);
}
export function noParseIntAtAll(raw: string): number {
  return Number(raw);
}
