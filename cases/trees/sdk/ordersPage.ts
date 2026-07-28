// SDK-driven consumer #2 — same `@acme/sdk` import fan-out. Three importing files clear the rule's
// minimum (a single dangling import proves nothing).
import { client } from '@acme/sdk';

export const orders = () => client.orders.list();
