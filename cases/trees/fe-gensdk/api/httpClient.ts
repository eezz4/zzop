// NEGATIVE CASE 2 of 2 — the generated-SDK shape. This is a vendored swagger-typescript-api `HttpClient`:
// the generated client's own source is IN the tree, so its call sites are read at their own sites
// (parser/parser-typescript/src/adapters/egress/generated_client.rs). What is not read is the BASE.
//
// parser/parser-typescript/src/adapters/client_base_generated.rs recognizes the base only as a
// STRING-LITERAL `baseUrl` class field on a class that also declares `request`:
//   "Only a string-literal value is recognized; a runtime/env base (`this.baseUrl = process.env.X`) or an
//    empty default (`baseUrl = \"\"`) yields nothing — never guessed, per the repo's IO convention."
//
// The field below is exactly that sentence's example, which is also what every deployed generated client
// looks like. The class qualifies (it declares `request`), so the recognizer inspects it and emits
// nothing — nothing in this file tells the engine the effective route of a descriptor path in ./Api.ts.
//
// The repair is on the other side: cases/zzop.config.jsonc declares this tree's `topology.clientBase`,
// and that statement, not this field, is what gives the calls their `/api`. This file stays as it is —
// it is the shape that makes the declaration necessary.
declare const process: { env: Record<string, string | undefined> };
declare function transport(url: string, init: unknown): Promise<unknown>;

export class HttpClient {
  public baseUrl: string = process.env.API_BASE_URL ?? '';

  public request = <T>(params: { path: string; method: string }): Promise<T> =>
    transport(this.baseUrl + params.path, { method: params.method }) as Promise<T>;
}
