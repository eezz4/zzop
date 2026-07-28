// DECOY for dead-exports / dead-candidates, variant 1: reachable ONLY through a re-export hop. Nothing
// imports this module directly — `./reexports.ts` re-exports its symbol and the tree barrel imports that.
// If the dep graph did not follow re-export edges, this file would read as an orphan; it must not.
export function chainedHelper(amountCents: number): number {
  return Math.max(0, amountCents);
}
