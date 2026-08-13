// The `callgraph` fixture's read handler WITHOUT its route registration line — see this tree's
// README for why the two halves are separated at all.
//
// Everything the call-graph-BFS rules need on the CODE side lives here: an exported handler symbol
// and, inside its body, a store WRITE (`db.create(...)`, receiver `db` matching the ORM receiver
// vocabulary). What is deliberately absent is the `apiRoutes.get('/stats', statsWithWrite)` line —
// that is the http PROVIDE, and the whole point of this tree is that the provide arrives from
// somewhere else (an overlay) so it can be withheld.
declare const db: { create(x: unknown): Promise<void> };

export async function statsWithWrite() {
  await db.create({ hit: 1 }); // WRITE reachable from a GET route once one is provided
  return { ok: true };
}
