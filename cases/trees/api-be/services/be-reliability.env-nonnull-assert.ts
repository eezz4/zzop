// be-reliability/env-nonnull-assert — bad: `process.env.X!` (also trips env-outside-config, since this is
// not a config module). good: read config from an injected object (no env access here).
export function bad() {
  const url = process.env.API_URL!;
  return url;
}

export function good(config: { apiUrl: string }) {
  return config.apiUrl;
}
