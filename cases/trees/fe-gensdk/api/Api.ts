// Emitted by swagger-typescript-api — the descriptor-object call shape (`generated-request-object-v1`)
// that the egress recognizer DOES read. Each `path` below is only the SUFFIX of the effective route; the
// prefix is HttpClient's `baseUrl`, which is not statically known. See ./httpClient.ts.
//
// (Deliberately carries no generated-file banner string: `vocabulary.generatedFileMarkers` changes how
// zzop treats a file, and the subject of this fixture is the base, not the banner.)
import { HttpClient } from './httpClient';

export class Api extends HttpClient {
  // Three same-method reads. They JOIN be-invoices, because this tree declares the base its generated
  // client carries in cases/zzop.config.jsonc (`topology.clientBase: "/api"`). Undeclared they were three
  // `prefix`-dimension route-near-misses — exactly `MIN_PREFIX_DRIFT_GROUP` — collapsing into one
  // `cross-layer/prefix-drift`, the degrade path zzop takes in place of a join it cannot make.
  invoices = {
    list: () => this.request({ path: '/invoices', method: 'GET' }),
    // THE DEFECT THE DECLARATION EXPOSED for this pair. be-invoices provides `GET /api/invoices/exports`
    // and no POST, so this call is a real cross-layer defect and `cross-layer/method-mismatch` fires here.
    // It could not before: the consume keyed as `POST /invoices/exports`, byte-identical to no provide
    // path. `cross-layer/unprovided-mutation-call` fires at this anchor before and after, so the repair
    // only ADDED a rule id here.
    requestExport: () => this.request({ path: '/invoices/exports', method: 'POST' }),
  };

  shipments = {
    list: () => this.request({ path: '/shipments', method: 'GET' }),
  };

  carriers = {
    list: () => this.request({ path: '/carriers', method: 'GET' }),
  };
}
