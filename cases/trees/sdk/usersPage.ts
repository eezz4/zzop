// SDK-driven consumer #1. API consumption flows through a generated client (`@acme/sdk`), so the egress
// extractor sees zero fetch-shaped http consumes — the cross-layer join is structurally blind here.
import { client } from '@acme/sdk';

export const users = () => client.users.list();
