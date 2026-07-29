// One of TWO backends deliberately implementing the SAME three routes (`be-catalog-b` is the other).
//
// This pair exists to reproduce the single most common shape in the real dogfood corpus, which nothing in
// this benchmark had: measured 2026-07-29 over 17 trees, FOUR front ends (fe-angular, fe-axios, fe-vite,
// fe-vue) each emitted 19 `cross-layer/ambiguous-consume` findings and ZERO edges, because ten sibling
// backends all implement the same RealWorld API. Every call matched many providers, the linker correctly
// refused to pick one, and the reader got 19 near-identical findings restating one per-tree fact.
//
// `cross-layer/all-consumes-unjoined` folds that. `fe-catalog` next door is the consumer.
declare const apiRoutes: { get(p: string, h: unknown): void; post(p: string, h: unknown): void };
declare const handler: unknown;

apiRoutes.get('/catalog/items', handler);
apiRoutes.get('/catalog/status', handler);
apiRoutes.post('/catalog/orders', handler);

export const marker = 1;
