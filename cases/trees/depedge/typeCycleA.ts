// A <-> B mutual `import type` cycle: each edge is a real fan-in edge (so neither is dead) but NOT a
// runtime cycle. Regression (type-only counted as a circular edge) = `circular` FALSE POSITIVE on both.
import type { TB } from './typeCycleB';
export interface TA {
  b: TB | null;
}
