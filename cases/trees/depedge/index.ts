// Entry (fanIn=0 -> exempt). Reaches the head of each indirection chain so the chained targets are
// live ONLY if the dep-edge fixes hold. Nothing here should ever fire a graph finding.
import { rused } from './reexportBarrel';
import { useT } from './typeConsumer';
import { Host } from './host';
import type { TA } from './typeCycleA';

export const app = {
  r: rused,
  t: useT((s) => s),
  host: Host,
};
export type Root = TA;
