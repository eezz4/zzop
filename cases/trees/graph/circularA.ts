// circular — circularA and circularB import each other (named imports create dep edges → an import
// cycle). Reachable from index, so this is a `circular` finding, not `unreachable`.
import { b } from './circularB';

export const a = b + 1;
