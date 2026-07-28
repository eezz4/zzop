// Fan-in for the router file (importing a router does NOT consume its procedures — consumption is a
// `.query()`/`.mutate()` client call, which this run has none of).
import { appRouter } from './router';

export const registry = { appRouter };
