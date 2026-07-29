// NEGATIVE CASE 1 of 2 — the `fe-axios` shape, planted OUTSIDE zzop's capability boundary on purpose.
//
// `axios.defaults.baseURL` becomes a tree-level consume prefix ONLY when its value is a string literal.
// parser/parser-typescript/src/adapters/client_base.rs says so in its own module doc:
//   "Only a string-literal (or zero-interpolation template) value is recognized;
//    `axios.defaults.baseURL = settings.baseApiUrl` or any other non-literal expression emits nothing,
//    per the repo's never-guess IO convention."
//
// The assignment below is exactly that sentence's example. No `client-base-prefix` sentinel is emitted,
// so nothing in this file tells the engine the effective route of a call in pages/articles.ts.
//
// This is not a bug report against the recognizer: refusing to guess a base is the right call. The
// repair is on the other side — the author STATES the base in cases/zzop.config.jsonc as
// `topology.clientBase`, and that declaration, not this line, is what makes the calls key `GET
// /api/articles` and join trees/be-articles. This file stays exactly as it is: it is the shape that
// makes the declaration necessary, and deleting the declaration must bring the whole miss back.
declare const axios: {
  defaults: { baseURL: string };
  get(url: string): Promise<unknown>;
  post(url: string, body: unknown): Promise<unknown>;
};

import { settings } from '../settings';

axios.defaults.baseURL = settings.baseApiUrl;

export const configured = true;
