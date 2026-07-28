// Other half of the dead cycle (imports deadA). fanIn>0 via deadA, never reached from index → unreachable.
import { a } from './deadA';

export const b = a;
