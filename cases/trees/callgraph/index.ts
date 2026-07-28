import { statsWithWrite } from './readEndpoint';
import { upsertRecord } from './writeEndpoint';

export const registry = { statsWithWrite, upsertRecord };
