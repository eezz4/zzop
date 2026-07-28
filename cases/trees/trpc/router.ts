// tRPC router fragment — provides a procedure that no source in this run calls (no `.query()`/`.mutate()`
// client consume anywhere) → cross-layer/unconsumed-procedure. The router factory is matched by lexical
// name (`router(...)` / `createTRPCRouter(...)`); a leaf is a `.query(...)`/`.mutation(...)` chain.
declare const router: (routes: Record<string, unknown>) => unknown;
declare const publicProcedure: { query(fn: unknown): unknown; mutation(fn: unknown): unknown };
declare const handler: unknown;

export const appRouter = router({
  ghost: publicProcedure.query(handler), // procedure `ghost` (query) — defined, never consumed
});
