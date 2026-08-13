// NEGATIVE CONTROL for code-hygiene/env-outside-config in the web tree — same role as api-be's.
// It reads process.env and must stay silent because it is the declared env-config module.
export const env = {
  publicApiBase: process.env.NEXT_PUBLIC_API_BASE ?? "",
};
