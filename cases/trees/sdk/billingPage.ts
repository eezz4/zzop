// SDK-driven consumer #3 — third `@acme/sdk` importer → cross-layer/sdk-import-no-visible-consume fires
// for source `sdk` (SDK imported from >= 3 files, < 5 statically visible http consumes).
import { client } from '@acme/sdk';

export const billing = () => client.billing.get();
