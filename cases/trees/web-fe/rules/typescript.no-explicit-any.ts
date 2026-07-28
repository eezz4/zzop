// typescript/no-explicit-any — bad: an `any` annotation. good: `unknown` (type-safe).
export function bad(x: any) {
  return x;
}

export function good(x: unknown) {
  return x;
}
