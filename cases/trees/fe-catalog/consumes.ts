// The consumer half of the ambiguity fixture — see be-catalog-a/provides.ts for the measured shape it
// stands for.
//
// All three calls use full literal paths and resolve cleanly: extraction is NOT the problem here, which is
// what separates this tree from `unresolved/`. The problem is that TWO sibling trees provide each key, so
// the linker refuses to draw an edge (picking one would be a guess) and all three land in
// `ambiguousConsumes`. Zero edges from this tree, three unjoined calls, none of them explained by a
// near-miss or a method/version mismatch — the exact condition `cross-layer/all-consumes-unjoined` folds.
//
// EXPECTED here is the FOLD, not the three per-call findings: `cross-layer/ambiguous-consume` would have
// fired once per line and is replaced. Disabling `cross-layer/all-consumes-unjoined` hands those three
// back, which is the contract that keeps the replacement from being a suppression.
declare function fetch(u: string, init?: unknown): Promise<unknown>;

export function loadCatalog() {
  fetch('/catalog/items');
  fetch('/catalog/status');
  fetch('/catalog/orders', { method: 'post' });
}
