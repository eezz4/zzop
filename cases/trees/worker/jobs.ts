// Second consumer of the `order` table — api-be already reads it (`getPrisma().order.findMany` in
// src/queries.ts, source `api`). A distinct source (`worker`) touching the SAME table is the
// shared-database coupling `cross-layer/shared-db-table` flags: a schema change to `order` in one
// service silently breaks the other, with no API contract in between.
declare function getPrisma(): { order: { findMany(a?: unknown): Promise<unknown> } };

export async function reconcile() {
  return getPrisma().order.findMany({}); // db-table consume `table:order` — also consumed by source `api`
}
