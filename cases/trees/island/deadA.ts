// Closed dead island: mutually imports deadB (each gives the other fanIn>0, so neither is a fanIn=0
// entrypoint) and no entrypoint reaches this pair → native `unreachable`. Named imports are USED so the
// fan-in edges actually exist.
import { b } from './deadB';
import { orphan } from './orphan';

export const a = b + orphan;
