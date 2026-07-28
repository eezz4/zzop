// Entry — named/namespace imports give every graph fixture fan-in (so nothing reads as dead) and make the
// circular pair reachable (a `circular` finding, not `unreachable`).
import { a } from './circularA';
import * as dup from './duplicateRoute';
import * as shadow from './routeShadowing';

export const registry = { a, dup, shadow };
