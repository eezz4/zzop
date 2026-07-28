// DECOY for sql/count-in-loop, sql/app-side-aggregation-reduce and sql/app-side-aggregation-filter-length.
// In scope, provably: `.tsx?` matches all three and their require_files (`\.count\s*\(`, `\.reduce\s*\(`,
// `\.filter\s*\(`) are each satisfied below, so all three scanned this file.
//
// What it probes is SCOPING. Both aggregation rules pair a `findMany()` with a `.reduce(` /
// `.filter().length` inside ONE method; `count-in-loop` fires only when the count call sits inside a loop
// span. Here the fetch lives in one method and the aggregation over its already-materialized result in
// another, and the count is a single round trip outside any loop — so a rule that widened from method
// scope to file scope, or that matched loop syntax by co-occurrence instead of by span, would surface
// here. (First draft of this decoy DID put the fetch and the reduce in one method and both rules fired
// correctly; that was a bad decoy, not a rule defect, and it was rewritten rather than labeled.)
export declare const prisma: {
  ledger: { count(a?: unknown): Promise<number>; findMany(a?: unknown): Promise<LedgerRow[]> };
};
export interface LedgerRow { id: string; amountCents: number; open: boolean }

export function loadOpenRows(): Promise<LedgerRow[]> {
  return prisma.ledger.findMany({ where: { open: true } });
}

export function countOpenRows(): Promise<number> {
  return prisma.ledger.count({ where: { open: true } });
}

export function summarize(rows: readonly LedgerRow[]): { total: number; open: number } {
  const total = rows.reduce((sum, row) => sum + row.amountCents, 0);
  return { total, open: rows.filter((row) => row.open).length };
}
