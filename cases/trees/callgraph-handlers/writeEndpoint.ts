// The `callgraph` fixture's write handler WITHOUT its route registration line — see this tree's
// README for why the two halves are separated at all.
//
// `upsertRecord` reaches a `create` (a retry duplicates the row) and calls no auth guard, which is
// what `non-idempotent-write` and `mutating-route-no-auth` respectively judge — but only once a
// PUT route naming this handler exists. That route is NOT here; it arrives as an overlay.
declare const db: { create(x: unknown): Promise<void> };

export async function upsertRecord() {
  await db.create({ v: 1 }); // create() under a PUT route, once one is provided
}
