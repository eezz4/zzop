// Entry (fanIn=0 → entrypoint). Reaches only `live`; the deadA<->deadB cycle and its orphan leaf are
// NOT reached from here → they are closed dead islands (fanIn>0 yet unreachable) → native `unreachable`.
import { live } from './live';

export const app = { live };
