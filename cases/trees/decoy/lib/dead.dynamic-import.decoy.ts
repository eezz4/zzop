// DECOY for dead-exports / dead-candidates, variant 2: reachable ONLY through a DYNAMIC import. No static
// import names this module anywhere in the tree — `./dead.dynamic-loader.ts` reaches it with `import(...)`
// at runtime. Code-split routes and lazily loaded plugins have exactly this shape, and reporting them as
// dead is the classic dead-code false positive.
export function lazilyLoadedPanel(): string {
  return 'panel';
}
