// NEGATIVE CONTROL for code-hygiene/env-outside-config. This file DOES read process.env — and must stay
// silent, because it is the project's declared env-config module (see ../../zzop-attributes.json).
// It exercises the exemption arm of the gate; the `bad` exports elsewhere exercise the firing arm.
export const env = {
  databaseUrl: process.env.DATABASE_URL ?? "",
  apiKey: process.env.API_KEY ?? "",
};
