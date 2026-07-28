// Reached ONLY through the bare re-export in reexportBarrel.ts (no direct importer). Gains fan-in via
// the re-export dep edge (reexport-edges-v1). Regression = fanIn 0 -> dead-candidates FALSE POSITIVE.
export const rused = 1;
